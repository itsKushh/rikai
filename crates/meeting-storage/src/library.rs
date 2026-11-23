use super::*;
#[derive(Clone, Debug)]
pub struct Hit {
    pub meeting: i64,
    pub segment: i64,
    pub start_ms: i64,
    pub text: String,
}
impl Library {
    pub fn hits(&self, query: &str) -> Result<Vec<Hit>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let term = format!("\"{}\"", query.trim().replace('"', "\"\""));
        let mut q=self.0.prepare("SELECT t.meeting_id,s.id,s.start_ms,snippet(speech_search,0,'«','»','…',16) FROM speech_search JOIN transcript_segment s ON s.id=speech_search.rowid JOIN transcript t ON t.id=s.transcript_id WHERE speech_search MATCH ? AND t.id=(SELECT max(v.id) FROM transcript v WHERE v.meeting_id=t.meeting_id) ORDER BY t.meeting_id DESC,s.start_ms LIMIT 100")?;
        Ok(q.query_map([term], |r| {
            Ok(Hit {
                meeting: r.get(0)?,
                segment: r.get(1)?,
                start_ms: r.get(2)?,
                text: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?)
    }
    pub fn rename_profile(&self, id: i64, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "Nom requis");
        self.0.execute(
            "UPDATE voice_profile SET name=?,updated_at=unixepoch() WHERE id=?",
            params![name.trim(), id],
        )?;
        Ok(())
    }
    pub fn metadata(&self, id: i64) -> Result<(String, String, String, String)> {
        Ok(self.0.query_row("SELECT title,project,description,coalesce((SELECT group_concat(tag,', ') FROM meeting_tag WHERE meeting_id=m.id),'') FROM meeting m WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?)
    }
    pub fn update_metadata(
        &mut self,
        id: i64,
        title: &str,
        project: &str,
        description: &str,
        tags: &str,
    ) -> Result<()> {
        ensure!(!title.trim().is_empty(), "Titre requis");
        let tx = self.0.transaction()?;
        tx.execute(
            "UPDATE meeting SET title=?,project=?,description=? WHERE id=?",
            params![title.trim(), project.trim(), description, id],
        )?;
        tx.execute("DELETE FROM meeting_tag WHERE meeting_id=?", [id])?;
        for tag in tags.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            tx.execute(
                "INSERT OR IGNORE INTO meeting_tag VALUES(?,?)",
                params![id, tag],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn filter(
        &self,
        meetings: &mut Vec<Meeting>,
        project_or_tag: &str,
        day: &str,
    ) -> Result<()> {
        if !day.trim().is_empty() {
            meetings.retain(|m| m.date.starts_with(day.trim()));
        }
        if !project_or_tag.trim().is_empty() {
            let v = project_or_tag.trim();
            let mut stmt = self
                .0
                .prepare("SELECT meeting_id FROM meeting_tag WHERE tag=? COLLATE NOCASE")?;
            let tags = stmt
                .query_map([v], |r| r.get(0))?
                .collect::<Result<std::collections::HashSet<i64>, _>>()?;
            meetings
                .retain(|m| tags.contains(&m.id) || m.project.to_lowercase() == v.to_lowercase());
        }
        Ok(())
    }
    pub fn versions(&self, id: i64) -> Result<Vec<(i64, String)>> {
        let mut q = self
            .0
            .prepare("SELECT id,body FROM summary WHERE meeting_id=? ORDER BY id DESC")?;
        Ok(q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?)
    }
}
