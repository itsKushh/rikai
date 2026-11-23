#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(any(windows, test))]
mod page;
#[cfg(windows)]
mod playback;
#[cfg(windows)]
mod ui;
#[cfg(any(windows, test))]
mod worker;
#[cfg(windows)]
fn main() {
    if let Err(e) = ui::run() {
        rfd::MessageDialog::new()
            .set_title("MeetingNotes")
            .set_description(format!("{e:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("MeetingNotes nécessite Windows x64, build 20348 ou ultérieur.");
    std::process::exit(1);
}
