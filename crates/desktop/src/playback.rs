use anyhow::{Result, ensure};
use std::path::Path;
use windows::{
    Win32::Media::Multimedia::{
        MCI_MODE_PLAY, MCI_STATUS, MCI_STATUS_ITEM, MCI_STATUS_MODE, MCI_STATUS_PARMS,
        MCI_STATUS_POSITION, MCI_WAIT, mciGetDeviceIDW, mciSendCommandW, mciSendStringW,
    },
    core::HSTRING,
};

fn command(s: &str) -> Result<()> {
    let code = unsafe { mciSendStringW(&HSTRING::from(s), None, None) };
    ensure!(code == 0, "Lecture audio impossible (MCI {code})");
    Ok(())
}
pub fn stop() {
    let _ = command("close mn_passage");
}
pub fn play(path: &Path, start_ms: i64, end_ms: i64) -> Result<()> {
    stop();
    ensure!(path.is_file(), "La piste audio est absente");
    command(&format!(
        "open \"{}\" type waveaudio alias mn_passage",
        path.display()
    ))?;
    let result = command("set mn_passage time format milliseconds")
        .and_then(|_| command(&format!("play mn_passage from {start_ms} to {end_ms}")));
    if result.is_err() {
        stop();
    }
    result
}

fn status(item: i32) -> Result<usize> {
    let mut params = MCI_STATUS_PARMS {
        dwItem: item as u32,
        ..Default::default()
    };
    let id = unsafe { mciGetDeviceIDW(&HSTRING::from("mn_passage")) };
    let err = unsafe {
        mciSendCommandW(
            id,
            MCI_STATUS,
            Some((MCI_STATUS_ITEM | MCI_WAIT) as usize),
            Some((&raw mut params) as usize),
        )
    };
    ensure!(err == 0, "État de lecture indisponible (MCI {err})");
    Ok(params.dwReturn)
}
pub fn position() -> Result<i64> {
    Ok(status(MCI_STATUS_POSITION)? as i64)
}
pub fn is_playing() -> Result<bool> {
    Ok(status(MCI_STATUS_MODE)? == MCI_MODE_PLAY as usize)
}
