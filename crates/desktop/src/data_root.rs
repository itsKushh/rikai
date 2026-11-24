use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn locate(profile: &Path) -> Result<PathBuf> {
    let root = profile.join("Rikai");
    let previous = profile.join("MeetingNotes");
    if !previous.is_dir() || root.exists() {
        return Ok(root);
    }
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(profile.join("Rikai-migration.lock"))?;
    guard.lock()?;
    if previous.is_dir() && !root.exists() {
        let session = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(previous.join("session.lock"))?;
        session
            .try_lock()
            .context("Fermez l’ancienne application avant de migrer la bibliothèque.")?;
        drop(session);
        fs::rename(previous, &root).context("Migration de la bibliothèque vers Rikai")?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_preserves_files_and_defers_while_recording() -> Result<()> {
        let profile = std::env::temp_dir().join(format!("rikai-migration-{}", std::process::id()));
        let old = profile.join("MeetingNotes");
        fs::create_dir_all(old.join("audio/1"))?;
        fs::write(old.join("audio/1/piste.wav"), b"recording")?;
        let active = fs::File::create(old.join("session.lock"))?;
        active.lock()?;
        assert!(locate(&profile).is_err());
        active.unlock()?;
        drop(active);
        let new = locate(&profile)?;
        assert_eq!(fs::read(new.join("audio/1/piste.wav"))?, b"recording");
        assert_eq!(locate(&profile)?, new);
        fs::remove_dir_all(profile)?;
        Ok(())
    }
}
