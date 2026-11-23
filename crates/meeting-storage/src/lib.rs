mod jobs;
mod library;
mod profiles;
use anyhow::{Result, ensure};
pub use library::Hit;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub project: String,
    pub status: String,
    pub duration_ms: i64,
    pub date: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub track: String,
    pub speaker: String,
    #[serde(default)]
    pub acoustic_label: String,
    pub echo_candidate: Option<serde_json::Value>,
    pub text: String,
    pub words: Vec<Word>,
    pub confidence: Option<f32>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Turn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub label: String,
}
pub struct Library(Connection);
impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        let mut c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON; PRAGMA journal_mode=WAL;",
        )?;
        let v: i32 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(v <= 3, "Version de bibliothèque trop récente");
        if v == 0 {
            let tx = c.transaction()?;
            tx.execute_batch(include_str!("../../../migrations/001_library.sql"))?;
            tx.commit()?;
        }
        if v < 2 {
            let tx = c.transaction()?;
            tx.execute_batch(include_str!("../../../migrations/002_profiles.sql"))?;
            tx.commit()?;
        }
        if v < 3 {
            let tx = c.transaction()?;
            tx.execute_batch(include_str!("../../../migrations/003_echo.sql"))?;
            tx.commit()?;
        }
        Ok(Self(c))
    }
    pub fn recover(&self) -> Result<()> {
        self.0.execute("UPDATE processing_job SET status='Interrompu',error='Fermeture inattendue' WHERE status='En cours'",[])?;
        self.0.execute(
            "UPDATE meeting SET status='Interrompue' WHERE status IN ('Capture','Traitement')",
            [],
        )?;
        Ok(())
    }
    pub fn create(&self, title: &str, project: &str, options: &str) -> Result<i64> {
        ensure!(!title.trim().is_empty(), "Le titre est requis");
        self.0.execute(
            "INSERT INTO meeting(title,project,status,options) VALUES(?,?,'Capture',?)",
            params![title.trim(), project, options],
        )?;
        Ok(self.0.last_insert_rowid())
    }
    pub fn options(&self, id: i64) -> Result<String> {
        Ok(self
            .0
            .query_row("SELECT options FROM meeting WHERE id=?", [id], |r| r.get(0))?)
    }
    pub fn finish(&self, id: i64, duration: i64, status: &str) -> Result<()> {
        self.0.execute(
            "UPDATE meeting SET ended_at=unixepoch(),duration_ms=?,status=? WHERE id=?",
            params![duration, status, id],
        )?;
        Ok(())
    }
    pub fn status(&self, id: i64, status: &str) -> Result<()> {
        self.0.execute(
            "UPDATE meeting SET status=? WHERE id=?",
            params![status, id],
        )?;
        Ok(())
    }
    pub fn track(&mut self, id: i64, name: &str, identity: &str, file: &str) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT INTO audio_source(meeting_id,name,identity_json) VALUES(?,?,?)",
            params![id, name, identity],
        )?;
        tx.execute(
            "INSERT INTO audio_track(meeting_id,source_id,file) VALUES(?,last_insert_rowid(),?)",
            params![id, file],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn pref(&self, k: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .query_row("SELECT value FROM preference WHERE key=?", [k], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn set_pref(&self, k: &str, v: &str) -> Result<()> {
        self.0.execute("INSERT INTO preference VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![k,v])?;
        Ok(())
    }
    pub fn list(&self, query: &str) -> Result<Vec<Meeting>> {
        let term = format!("\"{}\"", query.replace('"', "\"\""));
        let mut q=self.0.prepare("SELECT id,title,project,status,duration_ms,datetime(created_at,'unixepoch','localtime') FROM meeting m WHERE ?1='' OR EXISTS(SELECT 1 FROM meeting_speaker_assignment a LEFT JOIN voice_profile p ON p.id=a.profile_id WHERE a.meeting_id=m.id AND instr(lower(coalesce(a.display_name,p.name)),lower(?1))>0) OR id IN (SELECT rowid FROM title_search WHERE title_search MATCH ?2) OR id IN (SELECT t.meeting_id FROM speech_search f JOIN transcript_segment s ON f.rowid=s.id JOIN transcript t ON t.id=s.transcript_id WHERE speech_search MATCH ?2 AND t.id=(SELECT max(v.id) FROM transcript v WHERE v.meeting_id=t.meeting_id)) ORDER BY id DESC")?;
        Ok(q.query_map(params![query.trim(), term], |r| {
            Ok(Meeting {
                id: r.get(0)?,
                title: r.get(1)?,
                project: r.get(2)?,
                status: r.get(3)?,
                duration_ms: r.get(4)?,
                date: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?)
    }
    pub fn publish(
        &mut self,
        id: i64,
        settings: &str,
        parts: &[Segment],
        turns: &[Turn],
        candidates: std::collections::BTreeMap<String, profiles::Proposal>,
    ) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "DELETE FROM meeting_speaker_assignment WHERE meeting_id=?",
            [id],
        )?;
        profiles::proposals(&tx, id, candidates)?;
        let prior: Option<i64> = tx.query_row(
            "SELECT max(id) FROM transcript WHERE meeting_id=?",
            [id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO transcript(meeting_id,settings) VALUES(?,?)",
            params![id, settings],
        )?;
        let version = tx.last_insert_rowid();
        for s in parts {
            ensure!(
                s.start_ms >= 0 && s.end_ms >= s.start_ms,
                "Horodatage invalide"
            );
            tx.execute("INSERT INTO transcript_segment(transcript_id,track,start_ms,end_ms,speaker,raw,edited,words,confidence,echo_candidate) VALUES(?1,?2,?3,?4,?5,?6,(SELECT edited FROM transcript_segment WHERE transcript_id=?7 AND track=?2 AND start_ms=?3 AND end_ms=?4 AND raw=?6 LIMIT 1),?8,?9,?10)",params![version,s.track,s.start_ms,s.end_ms,s.speaker,s.text,prior,serde_json::to_string(&s.words)?,s.confidence,s.echo_candidate.as_ref().map(|v|v.to_string())])?;
        }
        for t in turns {
            tx.execute(
                "INSERT INTO speaker_turn(transcript_id,start_ms,end_ms,label) VALUES(?,?,?,?)",
                params![version, t.start_ms, t.end_ms, t.label],
            )?;
        }
        tx.execute("UPDATE meeting SET status='Transcrite' WHERE id=?", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn segments(&self, id: i64) -> Result<Vec<Segment>> {
        self.segments_at(id, None)
    }
    pub fn segments_at(&self, id: i64, version: Option<i64>) -> Result<Vec<Segment>> {
        let mut q=self.0.prepare("SELECT s.id,start_ms,end_ms,track,coalesce(a.display_name,p.name,s.speaker),coalesce(edited,raw),words,confidence,s.speaker,s.echo_candidate FROM transcript_segment s LEFT JOIN meeting_speaker_assignment a ON a.meeting_id=?1 AND s.transcript_id=(SELECT max(v.id) FROM transcript v WHERE v.meeting_id=?1) AND a.label=s.speaker LEFT JOIN voice_profile p ON p.id=a.profile_id WHERE transcript_id=(SELECT max(id) FROM transcript WHERE meeting_id=?1 AND (?2 IS NULL OR id=?2)) ORDER BY start_ms,end_ms,s.id")?;
        let mut rows = q.query(params![id, version])?;
        let mut out = vec![];
        while let Some(r) = rows.next()? {
            out.push(Segment {
                id: r.get(0)?,
                start_ms: r.get(1)?,
                end_ms: r.get(2)?,
                track: r.get(3)?,
                speaker: r.get(4)?,
                acoustic_label: r.get(8)?,
                echo_candidate: r
                    .get::<_, Option<String>>(9)?
                    .map(|s| serde_json::from_str(&s))
                    .transpose()?,
                text: r.get(5)?,
                words: serde_json::from_str(&r.get::<_, String>(6)?)?,
                confidence: r.get(7)?,
            })
        }
        Ok(out)
    }
    pub fn edit(&self, id: i64, text: &str) -> Result<()> {
        ensure!(
            self.0.execute(
                "UPDATE transcript_segment SET edited=? WHERE id=?",
                params![text, id]
            )? == 1,
            "Segment inconnu"
        );
        Ok(())
    }
    pub fn save_summary(
        &mut self,
        id: i64,
        body: &str,
        settings: &str,
        evidence: &[i64],
    ) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute("INSERT INTO summary(meeting_id,transcript_id,body,settings) VALUES(?1,(SELECT max(id) FROM transcript WHERE meeting_id=?1),?2,?3)",params![id,body,settings])?;
        let sid = tx.last_insert_rowid();
        for e in evidence {
            ensure!(tx.execute("INSERT OR IGNORE INTO summary_evidence SELECT ?1,s.id FROM transcript_segment s JOIN summary v ON v.transcript_id=s.transcript_id WHERE v.id=?1 AND s.id=?2",params![sid,e])?==1,"Référence de synthèse invalide");
        }
        tx.commit()?;
        Ok(())
    }
    pub fn summary(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .0
            .query_row(
                "SELECT body FROM summary WHERE meeting_id=? ORDER BY id DESC LIMIT 1",
                [id],
                |r| r.get(0),
            )
            .optional()?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_search_and_versions_survive_reopen() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "meetingnotes-storage-{}.sqlite",
            std::process::id()
        ));
        let mut db = Library::open(&path)?;
        let id = db.create("Réunion étude", "Produit", "{}")?;
        let s = Segment {
            id: 0,
            start_ms: 100,
            end_ms: 500,
            track: "mic.wav".into(),
            speaker: "Moi".into(),
            acoustic_label: "Moi".into(),
            echo_candidate: None,
            text: "maquette".into(),
            words: vec![],
            confidence: None,
        };
        db.publish(id, "{}", std::slice::from_ref(&s), &[], Default::default())?;
        db.edit(db.segments(id)?[0].id, "validation jeudi")?;
        db.publish(id, "{}", &[s], &[], Default::default())?;
        drop(db);
        let db = Library::open(&path)?;
        assert_eq!(db.list("étude")?[0].id, id);
        assert_eq!(db.list("jeudi")?.len(), 1);
        assert!(db.list("maquette")?.is_empty());
        assert_eq!(db.segments(id)?[0].text, "validation jeudi");
        drop(db);
        std::fs::remove_file(path)?;
        Ok(())
    }
}
