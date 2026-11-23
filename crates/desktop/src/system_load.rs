use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use sysinfo::System;

#[derive(Default)]
pub(super) struct Load {
    pub threads: u64,
    pub total_gib: u64,
    pub free_gib: f64,
    pub cpu: f32,
    pub gpu: Option<String>,
}

fn gpu_load() -> Option<String> {
    let primary = PathBuf::from(std::env::var_os("SystemRoot")?).join("System32/nvidia-smi.exe");
    let tool = if primary.is_file() {
        primary
    } else {
        PathBuf::from(std::env::var_os("ProgramFiles")?)
            .join("NVIDIA Corporation/NVSMI/nvidia-smi.exe")
    };
    let mut command = Command::new(tool);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args([
            "--id=0",
            "--query-gpu=name,utilization.gpu,memory.free,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        if child.try_wait().ok()?.is_some() {
            break;
        }
        if start.elapsed() >= Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let line = std::str::from_utf8(&output.stdout).ok()?.trim();
    let fields: Vec<_> = line.split(',').map(str::trim).collect();
    let [name, busy, free, total] = &fields[..] else {
        return None;
    };
    let (free, total) = (
        free.parse::<f64>().ok()? / 1024.,
        total.parse::<f64>().ok()? / 1024.,
    );
    Some(format!(
        "{name} · charge {busy} % · VRAM libre {free:.1} / {total:.1} Gio"
    ))
}

pub(super) fn monitor(events: mpsc::Sender<super::Event>) -> mpsc::SyncSender<()> {
    let (request, rx) = mpsc::sync_channel(0);
    thread::spawn(move || {
        let mut system = System::new();
        system.refresh_cpu_all();
        while rx.recv().is_ok() {
            system.refresh_cpu_all();
            system.refresh_memory();
            let value = Load {
                threads: system.cpus().len() as u64,
                total_gib: system.total_memory().div_ceil(1 << 30),
                free_gib: system.available_memory() as f64 / (1u64 << 30) as f64,
                cpu: system.global_cpu_usage(),
                gpu: gpu_load(),
            };
            if events.send(super::Event::Load(value)).is_err() {
                break;
            }
        }
    });
    request
}
