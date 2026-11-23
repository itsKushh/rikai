use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Deserialize, Serialize)]
pub struct Proposal {
    pub profile: i64,
    pub score: f64,
}
impl Library {
    pub fn profiles(&self, embeddings: bool) -> Result<Vec<Value>> {
        let mut stmt=self.0.prepare("SELECT id,name,model,CASE WHEN ? THEN embedding ELSE '[]' END FROM voice_profile ORDER BY name,id")?;
        let mut rows = stmt.query([embeddings])?;
        let mut out = vec![];
        while let Some(r) = rows.next()? {
            out.push(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"model":r.get::<_,String>(2)?,"embedding":serde_json::from_str::<Value>(&r.get::<_,String>(3)?)?}));
        }
        Ok(out)
    }
    pub fn add_profile(&self, v: &Value) -> Result<i64> {
        let name = v["name"].as_str().unwrap_or("").trim();
        ensure!(!name.is_empty(), "Le profil doit avoir un nom");
        self.0.execute(
            "INSERT INTO voice_profile(name,model,embedding,samples) VALUES(?,?,?,?)",
            params![
                name,
                v["model"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Modèle manquant"))?,
                v["embedding"].to_string(),
                v["samples"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Extrait manquant"))?
            ],
        )?;
        Ok(self.0.last_insert_rowid())
    }
    pub fn delete_profile(&self, id: i64) -> Result<()> {
        self.0
            .execute("DELETE FROM voice_profile WHERE id=?", [id])?;
        self.0.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
    pub fn suggestions(&self, id: i64) -> Result<Vec<(String, i64, String, f64)>> {
        let mut q=self.0.prepare("SELECT s.label,p.id,p.name,s.score FROM voice_suggestion s JOIN voice_profile p ON p.id=s.profile_id WHERE s.meeting_id=?")?;
        Ok(
            q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<_, _>>()?,
        )
    }
    pub fn name_speaker(&self, id: i64, label: &str, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Indiquez le nom de l’intervenant");
        self.0.execute("INSERT INTO meeting_speaker_assignment(meeting_id,label,display_name) VALUES(?,?,?) ON CONFLICT(meeting_id,label) DO UPDATE SET display_name=excluded.display_name,profile_id=NULL",params![id,label,name.trim()])?;
        Ok(())
    }
    pub fn confirm(&mut self, id: i64, label: &str, profile: Option<i64>) -> Result<()> {
        let tx = self.0.transaction()?;
        if let Some(p) = profile {
            tx.execute("INSERT INTO meeting_speaker_assignment(meeting_id,label,profile_id) VALUES(?,?,?) ON CONFLICT(meeting_id,label) DO UPDATE SET profile_id=excluded.profile_id,display_name=NULL",params![id,label,p])?;
        }
        tx.execute(
            "DELETE FROM voice_suggestion WHERE meeting_id=? AND label=?",
            params![id, label],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        self.0.execute("DELETE FROM meeting WHERE id=?", [id])?;
        Ok(())
    }
}

pub(super) fn proposals(
    tx: &rusqlite::Transaction,
    id: i64,
    p: BTreeMap<String, Proposal>,
) -> Result<()> {
    tx.execute("DELETE FROM voice_suggestion WHERE meeting_id=?", [id])?;
    for (label, v) in p {
        tx.execute(
            "INSERT INTO voice_suggestion VALUES(?,?,?,?)",
            params![id, label, v.profile, v.score],
        )?;
    }
    Ok(())
}
