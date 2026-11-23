use super::*;
impl Library {
    pub fn start_job(&mut self, id: i64, stage: &str) -> Result<i64> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT INTO processing_job(meeting_id,stage,status) VALUES(?,?,'En cours')",
            params![id, stage],
        )?;
        let job = tx.last_insert_rowid();
        tx.execute("UPDATE meeting SET status='Traitement' WHERE id=?", [id])?;
        tx.commit()?;
        Ok(job)
    }
    pub fn finish_job(&self, job: i64, error: Option<&str>) -> Result<()> {
        self.0.execute(
            "UPDATE processing_job SET status=?,error=? WHERE id=?",
            params![
                if error.is_some() {
                    "Interrompu"
                } else {
                    "Terminé"
                },
                error,
                job
            ],
        )?;
        Ok(())
    }
    pub fn transcript_versions(&self, id: i64) -> Result<Vec<i64>> {
        let mut q = self
            .0
            .prepare("SELECT id FROM transcript WHERE meeting_id=? ORDER BY id DESC")?;
        Ok(q.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?)
    }
}
