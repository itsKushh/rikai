use super::*;
impl Library {
    pub fn transcribed_ids(&self) -> Result<Vec<i64>> {
        Ok(self.0.prepare("SELECT DISTINCT meeting_id FROM transcript t WHERE id=(SELECT max(id) FROM transcript WHERE meeting_id=t.meeting_id) AND EXISTS(SELECT 1 FROM transcript_segment WHERE transcript_id=t.id)")?.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }
    pub fn projects(&self) -> Result<Vec<(i64, String)>> {
        Ok(self
            .0
            .prepare("SELECT id,name FROM project ORDER BY name COLLATE NOCASE")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn project_items(&self, project: i64) -> Result<Vec<i64>> {
        Ok(self
            .0
            .prepare(
                "SELECT meeting_id FROM project_item WHERE project_id=? ORDER BY meeting_id DESC",
            )?
            .query_map([project], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn save_project(&mut self, id: Option<i64>, name: &str, items: &[i64]) -> Result<i64> {
        let tx = self.0.transaction()?;
        let id = match id {
            Some(id) => id,
            None => {
                ensure!(!name.trim().is_empty(), "Donnez un nom au projet.");
                tx.execute("INSERT INTO project(name) VALUES(?)", [name.trim()])?;
                tx.last_insert_rowid()
            }
        };
        for item in items {
            tx.execute("INSERT INTO project_item(project_id,meeting_id) VALUES(?,?) ON CONFLICT DO NOTHING",params![id,item])?;
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn mark_clip(&self, id: i64, start: i64, end: i64) -> Result<()> {
        self.0.execute(
            "INSERT INTO marked_clip VALUES(?,?,?) ON CONFLICT DO NOTHING",
            params![id, start, end],
        )?;
        Ok(())
    }
    pub fn marked_clips(&self, id: i64) -> Result<Vec<(i64, i64)>> {
        Ok(self
            .0
            .prepare("SELECT start_ms,end_ms FROM marked_clip WHERE meeting_id=? ORDER BY end_ms")?
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn membership_and_clip_constraints() -> Result<()> {
        let mut db = Library::open(Path::new(":memory:"))?;
        let a = db.create("A", "Terrain", "{}")?;
        let b = db.create("B", "", "{}")?;
        let project = db.projects()?[0].0;
        db.save_project(Some(project), "", &[a, b])?;
        assert_eq!(db.project_items(project)?, vec![b, a]);
        assert!(db.save_project(None, "Autre", &[999]).is_err());
        assert_eq!(db.projects()?.len(), 1);
        db.mark_clip(a, 0, 800)?;
        db.mark_clip(a, 0, 800)?;
        assert_eq!(db.marked_clips(a)?, vec![(0, 800)]);
        assert!(db.mark_clip(a, 5, 2).is_err());
        Ok(())
    }
}
impl Library {
    pub fn archive_translation(
        &mut self,
        title: &str,
        project: &str,
        language: &str,
        lines: &[(i64, i64, String, String, String)],
    ) -> Result<i64> {
        ensure!(!lines.is_empty(), "Aucun passage traduit");
        let tx = self.0.transaction()?;
        tx.execute("INSERT INTO meeting(title,project,status,duration_ms,ended_at,options) VALUES(?,?,'Traduite',?,unixepoch(),?)",params![title,project,lines.iter().map(|s|s.1).max().unwrap_or_default(),serde_json::json!({"target":language,"kind":"live"}).to_string()])?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO transcript(meeting_id,settings) VALUES(?,'{}')",
            [id],
        )?;
        let version = tx.last_insert_rowid();
        for (start, end, speaker, original, translated) in lines {
            tx.execute("INSERT INTO transcript_segment(transcript_id,track,start_ms,end_ms,speaker,raw,words) VALUES(?,'',?,?,?,?,'[]')",params![version,start,end,speaker,original])?;
            tx.execute(
                "INSERT INTO segment_translation VALUES(?,?,?)",
                params![tx.last_insert_rowid(), language, translated],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }
}

/// Pair exact phrases shared by two transcripts and keep their source locations.
pub fn shared_phrases(left: &[Segment], right: &[Segment]) -> Vec<(String, usize, usize)> {
    use std::collections::{HashMap, HashSet};
    fn phrases(text: &str) -> Vec<String> {
        let lower = text.to_lowercase();
        let words: Vec<_> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        words
            .windows(3)
            .filter(|w| w.iter().filter(|s| s.chars().count() >= 4).count() >= 2)
            .map(|w| w.join(" "))
            .collect()
    }
    let mut first = HashMap::new();
    for (i, s) in left.iter().enumerate() {
        for phrase in phrases(&s.text) {
            let hit = first.entry(phrase).or_insert((i, 0usize));
            hit.1 += 1;
        }
    }
    let mut candidates = Vec::new();
    for (j, s) in right.iter().enumerate() {
        for phrase in phrases(&s.text) {
            if let Some(&(i, n)) = first.get(&phrase) {
                candidates.push((n, i, j, phrase));
            }
        }
    }
    candidates.sort();
    let mut used = HashSet::new();
    let mut found = Vec::new();
    for (_, i, j, phrase) in candidates {
        if used.insert((i, j)) {
            found.push((phrase, i, j));
            if found.len() == 6 {
                break;
            }
        }
    }
    found
}
