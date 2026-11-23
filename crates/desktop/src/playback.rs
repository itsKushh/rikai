use anyhow::{Result, ensure};
use std::path::Path;
use windows::{Win32::Media::Multimedia::mciSendStringW, core::HSTRING};

fn command(s: &str) -> Result<()> {
    let code = unsafe { mciSendStringW(&HSTRING::from(s), None, None) };
    ensure!(code == 0, "Lecture audio impossible (MCI {code})");
    Ok(())
}
pub fn stop() {
    let _ = command("close mn_passage");
}
pub fn play(path: &Path, start_ms: i64, end_ms: i64) -> Result<()> {
    ensure!(path.is_file(), "La piste audio est absente");
    stop();
    command(&format!(
        "open \"{}\" type waveaudio alias mn_passage",
        path.display()
    ))?;
    command("set mn_passage time format milliseconds")?;
    command(&format!("play mn_passage from {start_ms} to {end_ms}"))
}
