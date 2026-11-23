use super::*;
use anyhow::{anyhow, bail};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};
use windows::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::{FILETIME, HANDLE, WAIT_TIMEOUT},
        System::{
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
            SystemInformation::OSVERSIONINFOW,
            Threading::{
                GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
                WaitForSingleObject,
            },
        },
    },
    core::{Owned, PWSTR},
};

pub fn check_windows() -> Result<u32> {
    let mut v = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    unsafe { RtlGetVersion(&mut v).ok()? };
    ensure!(
        v.dwBuildNumber >= 20348,
        "Capture ciblée indisponible sur le build {}. Windows build 20348 ou ultérieur requis.",
        v.dwBuildNumber
    );
    Ok(v.dwBuildNumber)
}
struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        wasapi::initialize_mta().ok()?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

fn process_handle(pid: u32) -> Result<(Owned<HANDLE>, u64)> {
    let h = unsafe {
        Owned::new(OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )?)
    };
    let (mut born, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(*h, &mut born, &mut exit, &mut kernel, &mut user)?;
    }
    Ok((
        h,
        (u64::from(born.dwHighDateTime) << 32) | u64::from(born.dwLowDateTime),
    ))
}
fn verify_image(handle: HANDLE, expected: &Path) -> Result<()> {
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )?;
    }
    let actual = String::from_utf16(&path[..length as usize])?;
    ensure!(
        std::path::Path::new(&actual) == expected,
        "Le processus a changé depuis la détection. Actualisez les sources."
    );
    Ok(())
}
fn processes() -> System {
    System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
    )
}
pub fn discover() -> Result<Vec<Input>> {
    check_windows()?;
    let _com = Apartment::enter()?;
    let sys = processes();
    let devs = DeviceEnumerator::new()?;
    let mut apps = HashMap::new();
    let mut inputs = Vec::new();
    for dev in &devs.get_device_collection(Direction::Render)? {
        let sessions = dev?
            .get_iaudiosessionmanager()?
            .get_audiosessionenumerator()?;
        for i in 0..sessions.get_count()? {
            let s = sessions.get_session(i)?;
            let Some(mut proc) = sys.process(Pid::from_u32(s.get_process_id()?)) else {
                continue;
            };
            let Some(path) = proc.exe() else { continue };
            if proc.pid().as_u32() == 0 || path.as_os_str().is_empty() {
                continue;
            }
            let mut seen = HashSet::new();
            while let Some(p) = proc.parent().and_then(|p| sys.process(p)) {
                if !seen.insert(p.pid())
                    || p.exe() != Some(path)
                    || p.start_time() > proc.start_time()
                {
                    break;
                }
                proc = p;
            }
            let pid = proc.pid().as_u32();
            let Ok((_, born)) = process_handle(pid) else {
                continue;
            };
            let peak = s
                .get_audiometerinformation()
                .and_then(|m| m.get_peak_value())
                .unwrap_or_default();
            let entry = apps.entry(pid).or_insert_with(|| Input {
                name: proc.name().to_string_lossy().into_owned(),
                peak: 0.,
                source: Source::Application {
                    pid,
                    born,
                    exe: path.to_owned(),
                },
            });
            entry.peak = entry.peak.max(peak);
        }
    }
    inputs.extend(apps.into_values());
    inputs.sort_by_key(|i| i.name.to_lowercase());
    for d in &devs.get_device_collection(Direction::Capture)? {
        let d = d?;
        inputs.push(Input {
            name: d.get_friendlyname()?,
            source: Source::Microphone {
                endpoint: d.get_id()?,
            },
            peak: d
                .get_audiometerinformation()
                .and_then(|m| m.get_peak_value())
                .unwrap_or_default(),
        });
    }
    Ok(inputs)
}

fn now() -> u64 {
    let (mut count, mut freq) = (0, 0);
    // QPC est disponible sur toutes les versions Windows prises en charge.
    unsafe {
        QueryPerformanceCounter(&mut count).unwrap();
        QueryPerformanceFrequency(&mut freq).unwrap();
    }
    (count as u128 * u128::from(TICKS) / freq as u128) as u64
}
#[derive(Clone)]
pub struct Control {
    timeline: Arc<Mutex<Timeline>>,
    incidents: Arc<Mutex<HashMap<String, String>>>,
    ready: Arc<AtomicUsize>,
    levels: Arc<Mutex<HashMap<String, f32>>>,
    gains: Arc<Mutex<HashMap<Source, f32>>>,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            timeline: Arc::new(Mutex::new(Timeline::new(now()))),
            incidents: Default::default(),
            ready: Default::default(),
            levels: Default::default(),
            gains: Default::default(),
        }
    }
}
impl Control {
    pub fn set_gain_db(&self, source: &Source, db: i8) {
        let db = db.clamp(MIN_GAIN_DB, MAX_GAIN_DB);
        self.gains
            .lock()
            .unwrap()
            .insert(source.clone(), 10f32.powf(f32::from(db) / 20.));
    }
    pub fn stop(&self) {
        self.timeline.lock().unwrap().stop(now());
    }
    pub fn pause(&self) {
        self.timeline.lock().unwrap().pause(now());
    }
    pub fn resume(&self) {
        self.timeline.lock().unwrap().resume(now());
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.timeline.lock().unwrap().frames(now()) * 1000 / u64::from(RATE)
    }
    pub fn ready(&self) -> usize {
        self.ready.load(Ordering::Acquire)
    }
    pub fn incidents(&self) -> HashMap<String, String> {
        self.incidents.lock().unwrap().clone()
    }
    pub fn levels(&self) -> HashMap<String, f32> {
        self.levels.lock().unwrap().clone()
    }
}
fn validate(inputs: &[Input]) -> Result<()> {
    ensure!(!inputs.is_empty(), "Sélectionnez au moins une source");
    let sys = processes();
    let mut roots = HashSet::new();
    let mut mic = false;
    for input in inputs {
        match &input.source {
            Source::Application { pid, .. } => {
                ensure!(
                    roots.insert(Pid::from_u32(*pid)),
                    "Application sélectionnée deux fois"
                );
            }
            Source::Microphone { .. } => {
                ensure!(!mic, "Sélectionnez un seul microphone");
                mic = true;
            }
        }
    }
    for pid in &roots {
        let mut p = sys
            .process(*pid)
            .context("Application fermée. Actualisez les sources.")?;
        let mut seen = HashSet::new();
        while let Some(parent) = p.parent().and_then(|id| sys.process(id)) {
            if !seen.insert(parent.pid()) || parent.start_time() > p.start_time() {
                break;
            }
            ensure!(
                !roots.contains(&parent.pid()),
                "Deux sources appartiennent à la même arborescence"
            );
            p = parent;
        }
    }
    Ok(())
}
/// Appeler depuis un thread de travail. Le dossier doit être nouveau.
pub fn record(dir: &Path, inputs: Vec<Input>, ctl: Control) -> Result<Manifest> {
    check_windows()?;
    validate(&inputs)?;
    std::fs::create_dir(dir)?;
    let manifest = Manifest {
        version: 1,
        rate: RATE,
        tracks: make_tracks(inputs),
    };
    std::fs::write(
        dir.join("sources.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    if let Err(e) = capture_tracks(Some(dir), &manifest.tracks, &ctl) {
        std::fs::write(dir.join("erreur.txt"), format!("{e:#}"))?;
        return Err(e);
    }
    mix_apps(dir, &manifest.tracks)?;
    std::fs::write(dir.join("finalise"), ctl.elapsed_ms().to_string())?;
    Ok(manifest)
}

/// Mesure les mêmes flux et applique le même gain que l'enregistreur, en mémoire.
pub fn test_levels(inputs: Vec<Input>, ctl: Control) -> Result<()> {
    check_windows()?;
    validate(&inputs)?;
    capture_tracks(None, &make_tracks(inputs), &ctl)
}

fn make_tracks(inputs: Vec<Input>) -> Vec<Track> {
    inputs
        .into_iter()
        .enumerate()
        .map(|(i, input)| Track {
            file: format!("piste-{}.wav", i + 1),
            input,
        })
        .collect()
}
fn capture_tracks(dir: Option<&Path>, tracks: &[Track], ctl: &Control) -> Result<()> {
    thread::scope(|scope| {
        let handles: Vec<_> = tracks
            .iter()
            .map(|t| {
                let ctl = ctl.clone();
                scope.spawn(move || {
                    let result = capture_one(dir, t, &ctl)
                        .with_context(|| format!("Source « {} »", t.input.name));
                    if result.is_err() {
                        ctl.stop();
                    }
                    result
                })
            })
            .collect();
        for h in handles {
            h.join()
                .map_err(|_| anyhow!("Interruption du thread audio"))??;
        }
        Ok(())
    })
}
fn capture_one(dir: Option<&Path>, track: &Track, ctl: &Control) -> Result<()> {
    let mut wav = dir
        .map(|dir| TrackWriter::create(&dir.join(&track.file)))
        .transpose()?;
    let mut log: Box<dyn Write> = match dir {
        Some(dir) => Box::new(File::create_new(dir.join(format!("{}.log", track.file)))?),
        None => Box::new(std::io::sink()),
    };
    let mut opened = false;
    let mut deadline = None;
    loop {
        if ctl.timeline.lock().unwrap().stop.is_some() {
            break;
        }
        let result = capture_stream(track, ctl, wav.as_mut(), &mut log, &mut opened);
        match result {
            Ok(()) => break,
            Err(e) if opened && matches!(track.input.source, Source::Microphone { .. }) => {
                let message =
                    format!("Microphone indisponible, reconnexion à la même entrée : {e}");
                ctl.incidents
                    .lock()
                    .unwrap()
                    .insert(track.file.clone(), message.clone());
                writeln!(log, "{message}")?;
                if Instant::now()
                    >= *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(15))
                {
                    return Err(e);
                }
                thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(wav) = wav {
        let clock = ctl.timeline.lock().unwrap().clone();
        wav.finish(clock.frames(now()))?;
    }
    Ok(())
}
fn capture_stream(
    track: &Track,
    ctl: &Control,
    mut wav: Option<&mut TrackWriter>,
    log: &mut dyn Write,
    opened: &mut bool,
) -> Result<()> {
    let _com = Apartment::enter()?;
    let mut pinned = None;
    let mut client = match &track.input.source {
        Source::Application { pid, born, exe } => {
            let (handle, actual) = process_handle(*pid)?;
            ensure!(
                actual == *born,
                "Le processus a redémarré. Sélectionnez-le à nouveau."
            );
            verify_image(*handle, exe)?;
            pinned = Some(handle);
            AudioClient::new_application_loopback_client(*pid, true)?
        }
        Source::Microphone { endpoint } => {
            let dev = DeviceEnumerator::new()?.get_device(endpoint)?;
            ensure!(
                dev.get_direction() == Direction::Capture,
                "Le périphérique choisi doit être une entrée"
            );
            dev.get_iaudioclient()?
        }
    };
    let fmt = WaveFormat::new(16, 16, SampleType::Int, RATE as usize, 1, None);
    client.initialize_client(
        &fmt,
        Direction::Capture,
        StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 0,
        },
        None,
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;

    let mut buf = Vec::new();
    client.start_stream()?;
    if !*opened {
        ctl.ready.fetch_add(1, Ordering::Release);
        *opened = true;
    }
    ctl.incidents.lock().unwrap().remove(&track.file);
    let mut last_packet = Instant::now();
    let mut held_peak = 0.;
    let mut peak_time = Instant::now();
    let mut prev_end = None;
    loop {
        let _ = event.wait_for_event(50);
        let clock = ctl.timeline.lock().unwrap().clone();
        if pinned
            .as_ref()
            .is_some_and(|h| unsafe { WaitForSingleObject(**h, 0) != WAIT_TIMEOUT })
        {
            bail!("Application fermée. Pistes partielles conservées.");
        }
        while let Some(n) = capture.get_next_packet_size()?.filter(|&n| n > 0) {
            buf.resize(2 * n as usize, 0);
            let (frames, info) = capture.read_from_device(&mut buf)?;
            let bytes = &mut buf[..frames as usize * 2];
            let duration = u64::from(frames) * TICKS / u64::from(RATE);
            let at = if info.flags.timestamp_error || info.timestamp == 0 {
                writeln!(log, "Horodatage estimé, position {}", info.index)?;
                prev_end.unwrap_or_else(|| now().saturating_sub(duration))
            } else {
                info.timestamp
            };
            if info.flags.data_discontinuity {
                writeln!(log, "Discontinuité QPC {at}, position {}", info.index)?;
            }
            ensure!(
                at <= now() + TICKS,
                "Horodatage de périphérique incohérent. Capture interrompue."
            );
            prev_end = Some(at + duration);
            let gain = ctl
                .gains
                .lock()
                .unwrap()
                .get(&track.input.source)
                .copied()
                .unwrap_or(1.);
            let peak = apply_gain(bytes, gain);
            if let Some(writer) = wav.as_deref_mut() {
                writer.packet(at, bytes, &clock)?;
            }
            // Maintenir une crête au moins un rafraîchissement de l'interface.
            if peak >= held_peak
                || peak_time.elapsed() >= Duration::from_millis(300)
                || clock.paused()
            {
                held_peak = if clock.paused() { 0. } else { peak };
                peak_time = Instant::now();
                ctl.levels
                    .lock()
                    .unwrap()
                    .insert(track.file.clone(), held_peak);
            }
            last_packet = Instant::now();
        }
        if last_packet.elapsed() > Duration::from_millis(300) {
            held_peak = 0.;
            ctl.levels.lock().unwrap().insert(track.file.clone(), 0.);
        }
        if clock.stop.is_some() {
            client.stop_stream()?;
            break;
        }
    }
    Ok(())
}
