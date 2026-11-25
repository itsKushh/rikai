use crate::{i18n::tr, page::Page};
#[path = "appearance.rs"]
mod appearance;
use anyhow::{Context as _, Result, ensure};
use appearance::Palette;
use audio_capture::{Control, Input, Manifest, Source, discover, record};
use gpui::{prelude::*, *};
use gpui_ce_elements::editable_text::{
    EditableTextState, StringStorage,
    actions::{DEFAULT_INPUT_CONTEXT, default_bindings},
    text_area, text_input,
};
use meeting_storage::{Library, Meeting, Segment};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub fn run() -> Result<()> {
    let build = audio_capture::check_windows()?;
    let profile =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("Profil Windows introuvable")?);
    let root = crate::data_root::locate(&profile)?;
    fs::create_dir_all(root.join("audio"))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("session.lock"))?;
    lock.try_lock()
        .context("Une autre instance utilise déjà la bibliothèque")?;
    let log_path = root.join("technique.log");
    if log_path.metadata().is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
        fs::rename(&log_path, root.join("technique-precedent.log")).ok();
    }
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(log))
        .try_init();
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(location) = info.location() {
            tracing::error!(
                file = location.file(),
                line = location.line(),
                "Panique Rust"
            );
        }
        previous_hook(info);
    }));
    tracing::info!(build, "Ouverture de la bibliothèque");
    let db = Library::open(&root.join("reunions.sqlite3"))?;
    db.recover()?;
    gpui_platform::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx| {
            let _ = cx.text_system().add_fonts(vec![
                std::borrow::Cow::Borrowed(include_bytes!("../../../assets/inter/Inter.ttf")),
                std::borrow::Cow::Borrowed(include_bytes!(
                    "../../../assets/scripts-ui/RikaiScriptUI.otf"
                )),
            ]);
            cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
            let opt = view::window_frame::options(
                "Rikai",
                size(px(1120.), px(740.)),
                size(px(640.), px(440.)),
                cx,
            );
            cx.open_window(opt, move |w, cx| {
                let button = taskbar::DownloadButton::attach(w);
                let desk = cx.new(move |cx| Desk::new(root, db, button, cx));
                let weak = desk.downgrade();
                w.on_window_should_close(cx, move |_, cx| {
                    weak.update(cx, Desk::can_close).unwrap_or(true)
                });
                desk
            })
            .expect("Ouverture de la fenêtre Rikai");
            cx.activate(true);
        });
    drop(lock);
    Ok(())
}
use view::live;
#[path = "services.rs"]
mod services;
#[path = "system_load.rs"]
mod system_load;
#[path = "taskbar.rs"]
mod taskbar;
enum Event {
    Load(system_load::Load),
    GpuTest(Result<Value>),
    Hardware(String, Value, bool, Value),
    StorageChanged(Result<Value>),
    Translated(i64, String, String, Result<Value>),
    Update(&'static str, Result<Value>),
    Recovered(i64, Result<(usize, usize)>),
    Installed(Result<Value>),
    Enrolled(i64, String, Result<Value>),
    Sources(Result<Vec<Input>>),
    Recorded(i64, Result<Manifest>),
    Stage(super::worker::Progress),
    LevelsTested(Result<()>),
    Processed(i64, bool, Value, Result<Value>),
}
type Field = Entity<EditableTextState>;
fn field(cx: &mut Context<Desk>) -> Field {
    cx.new(|cx| EditableTextState::new(StringStorage::default(), cx))
}
type ConfirmAction = Box<dyn FnOnce(&mut Desk) -> Result<()>>;
struct Confirmation {
    title: &'static str,
    detail: String,
    accept: &'static str,
    action: ConfirmAction,
}
struct LevelTest {
    ctl: Control,
    tracks: Vec<Source>,
    stopping: bool,
    start_after: bool,
}

#[derive(Clone, Copy)]
struct PlayingSegment {
    id: i64,
    position: i64,
    end: i64,
    paused: bool,
}

struct Desk {
    taskbar: Option<taskbar::DownloadButton>,
    live_window: Option<WindowHandle<live::Live>>,
    browse_query: Field,
    browse_term: String,
    excerpts: Vec<meeting_storage::Excerpt>,
    section: usize,
    installed: Value,
    storage: Value,
    nvidia_ready: bool,
    language_search: Field,
    language_picker: Option<i64>,
    pending_translation: Option<(i64, String)>,
    translations: HashMap<i64, Vec<(String, String)>>,
    pending_summary: bool,
    update_info: Option<Value>,
    update_status: String,
    update_checking: bool,
    update_ready: bool,
    update_downloading: bool,
    release_focus: FocusHandle,
    release_was_open: bool,
    confirmation: Option<Confirmation>,
    hardware: String,
    recommended: String,
    monitor: mpsc::SyncSender<()>,
    last_measure: Option<Instant>,
    load: system_load::Load,
    budget_drag: Option<(view::budgets::Budget, Bounds<Pixels>)>,
    vocabulary: Field,
    root: PathBuf,
    db: Library,
    page: Page,
    settings_origin: Page,
    drawer_focus: FocusHandle,
    drawer_was_open: bool,
    inspector: Inspector,
    menu: Option<Preference>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    inputs: Vec<Input>,
    chosen: HashSet<Source>,
    scanning: bool,
    last_scan: Instant,
    initial: bool,
    ctl: Option<Control>,
    capture_names: Vec<String>,
    preview: Option<LevelTest>,
    gains: HashMap<Source, i8>,
    work_started: Instant,
    last_progress: Instant,
    transfer: Option<(u64, u64)>,
    generated_chars: Option<u64>,
    paused: bool,
    stopping: bool,
    cancel: Option<Arc<AtomicBool>>,
    job: Option<i64>,
    summary_only: bool,
    id: i64,
    title: Field,
    project: Field,
    search: Field,
    edit: Field,
    edit_id: Option<i64>,
    profile_name: Field,
    profiles: Vec<Value>,
    suggestions: Vec<(String, i64, String, f64)>,
    self_name: Field,
    endpoint: Field,
    remote_model: Field,
    filter: Field,
    day: Field,
    tags: Field,
    description: Field,
    metadata: bool,
    hits: Vec<meeting_storage::Hit>,
    dialogues: ListState,
    playing: Option<PlayingSegment>,
    report_list: ListState,
    report_lines: Vec<String>,
    summary_ids: Vec<i64>,
    transcript_ids: Vec<i64>,
    reading_version: [Option<i64>; 2],
    record_menu: Option<RecordMenu>,
    scroll_grab: Option<Pixels>,
    tab_geometry: [std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>; 4],
    discard: bool,
    library: Vec<Meeting>,
    segments: Vec<Segment>,
    summary: String,
    tab: u8,
    settings: Value,
    message: String,
    stage: String,
}
impl Desk {
    fn can_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.busy() {
            self.message = "Arrêtez la capture ou annulez le traitement avant de fermer.".into();
            cx.notify();
            return false;
        }
        true
    }

    fn new(
        root: PathBuf,
        db: Library,
        taskbar: Option<taskbar::DownloadButton>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut settings: Value = db
            .pref("settings")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({"language":"fr","device":"cpu","self_name":"Moi"}));
        if let Some(parent) = root.parent() {
            let legacy = parent.join("MeetingNotes");
            let mut changed = false;
            for key in [
                "whisper",
                "gguf",
                "embedding",
                "segmentation",
                "translation",
            ] {
                if let Some(rel) = settings[key]
                    .as_str()
                    .and_then(|v| std::path::Path::new(v).strip_prefix(&legacy).ok())
                {
                    let path = root.join(rel);
                    if path.exists() {
                        settings[key] = json!(path);
                        changed = true;
                    }
                }
            }
            if changed {
                if let Err(err) = db.set_pref("settings", &settings.to_string()) {
                    tracing::warn!(%err,"Migration des chemins");
                }
            }
        }
        crate::i18n::select(settings["locale"].as_str());
        cx.set_global(Palette::new(settings["light"] == true));
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_list(sysinfo::CpuRefreshKind::everything());
        let ram = sys.available_memory() / 1024 / 1024 / 1024;
        let cpu = sys
            .cpus()
            .first()
            .map(|v| v.brand())
            .unwrap_or("Processeur inconnu");
        let hardware = format!("{cpu} · {ram} Gio de RAM disponibles");
        let recommended = if ram < 6 { "Rapide" } else { "Équilibré" }.to_string();
        let tx_hw = tx.clone();
        thread::spawn(move || {
            let res = super::worker::run(
                0,
                "capabilities",
                json!({}),
                Arc::new(AtomicBool::new(false)),
                |_| {},
            );
            let gpu = match &res {
                Ok(v) if v["cuda"].as_u64().unwrap_or(0) > 0 => "GPU NVIDIA détecté",
                Ok(_) => "Calcul sur processeur",
                Err(_) => "Worker indisponible",
            };
            let caps = res.unwrap_or_default();
            let _ = tx_hw.send(Event::Hardware(
                gpu.into(),
                caps["installed"].clone(),
                caps["runtime"]["cublas_major"] == 12,
                caps["storage"].clone(),
            ));
        });
        let mut s = Self {
            taskbar,
            live_window: None,
            browse_query: field(cx),
            browse_term: String::new(),
            excerpts: Vec::new(),
            section: 0,
            installed: json!({}),
            storage: Value::Null,
            nvidia_ready: false,
            language_search: field(cx),
            language_picker: None,
            pending_translation: None,
            translations: HashMap::new(),
            pending_summary: false,
            update_info: None,
            update_status: String::new(),
            update_checking: false,
            update_ready: false,
            update_downloading: false,
            release_focus: cx.focus_handle(),
            release_was_open: false,
            confirmation: None,
            hardware,
            monitor: system_load::monitor(tx.clone()),
            last_measure: None,
            load: Default::default(),
            budget_drag: None,
            vocabulary: field(cx),
            recommended,
            root,
            db,
            page: Page::Home,
            settings_origin: Page::Home,
            drawer_focus: cx.focus_handle(),
            drawer_was_open: false,
            inspector: Inspector::Session,
            menu: None,
            tx,
            rx,
            inputs: vec![],
            chosen: HashSet::new(),
            scanning: false,
            last_scan: Instant::now(),
            initial: true,
            job: None,
            summary_only: false,
            ctl: None,
            capture_names: vec![],
            preview: None,
            gains: HashMap::new(),
            work_started: Instant::now(),
            last_progress: Instant::now(),
            transfer: None,
            generated_chars: None,
            paused: false,
            stopping: false,
            cancel: None,
            id: 0,
            title: field(cx),
            project: field(cx),
            search: field(cx),
            edit: field(cx),
            edit_id: None,
            profile_name: field(cx),
            profiles: vec![],
            suggestions: vec![],
            self_name: field(cx),
            endpoint: field(cx),
            remote_model: field(cx),
            filter: field(cx),
            day: field(cx),
            tags: field(cx),
            description: field(cx),
            metadata: false,
            hits: vec![],
            dialogues: ListState::new(0, ListAlignment::Top, px(180.)),
            playing: None,
            report_list: ListState::new(0, ListAlignment::Top, px(180.)),
            report_lines: Vec::new(),
            summary_ids: Vec::new(),
            transcript_ids: Vec::new(),
            reading_version: [None, None],
            record_menu: None,
            scroll_grab: None,
            tab_geometry: Default::default(),
            discard: false,
            library: vec![],
            segments: vec![],
            summary: String::new(),
            tab: 0,
            settings,
            message: String::new(),
            stage: String::new(),
        };
        for (f, k) in [
            (&s.vocabulary, "vocabulary"),
            (&s.self_name, "self_name"),
            (&s.endpoint, "endpoint"),
            (&s.remote_model, "remote_model"),
        ] {
            if let Some(v) = s.settings[k].as_str() {
                f.update(cx, |f, cx| f.emplace(v, cx));
            }
        }
        cx.observe(&s.language_search, |_, _, cx| cx.notify())
            .detach();
        cx.observe(&s.browse_query, |s, f, cx| {
            let query = f.read(cx).as_str();
            if query == s.browse_term {
                return;
            }
            s.browse_term = query.to_owned();
            match s.db.excerpts(query) {
                Ok(rows) => s.excerpts = rows,
                Err(e) => s.message = e.to_string(),
            }
            cx.notify();
        })
        .detach();
        s.update_request("check_update");
        cx.observe(&s.vocabulary, |_, _, cx| cx.notify()).detach();
        s.excerpts = s.db.excerpts("").unwrap_or_default();
        if let Err(e) = s.refresh("") {
            s.message = e.to_string();
        }
        cx.spawn(async move |this, cx| {
            let mut delay = 200;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(delay))
                    .await;
                let Ok(next) = this.update(cx, |s, cx| {
                    s.tick(cx);
                    if s.playing.is_some_and(|p| !p.paused) {
                        50
                    } else {
                        200
                    }
                }) else {
                    break;
                };
                delay = next;
            }
        })
        .detach();
        s
    }
    fn stop_segment(&mut self) {
        self.playing = None;
        super::playback::stop();
    }
    fn listen(&mut self, id: i64, from: Option<f32>) -> Result<()> {
        let seg = self
            .segments
            .iter()
            .find(|s| s.id == id)
            .context("Passage introuvable")?;
        let (start, end) = (seg.start_ms, seg.end_ms);
        ensure!(end > start, "Ce passage est vide");
        let current = self.playing.filter(|p| p.id == id);
        if let Some(mut p) = current
            && !p.paused
            && from.is_none()
        {
            p.position = super::playback::position()?.clamp(start, end);
            super::playback::stop();
            p.paused = true;
            self.playing = Some(p);
            return Ok(());
        }
        let pos = from
            .map(|f| start + ((end - start) as f64 * f64::from(f.clamp(0., 1.))) as i64)
            .or(current.map(|p| p.position))
            .unwrap_or(start)
            .clamp(start, end - 1);
        let path = self.audio_dir(self.id).join(&seg.track);
        self.playing = None;
        super::playback::play(&path, pos, end)?;
        self.playing = Some(PlayingSegment {
            id,
            position: pos,
            end,
            paused: false,
        });
        Ok(())
    }
    fn save_clip(&self, id: i64, w: &Window, cx: &mut Context<Self>) -> Result<()> {
        let seg = self
            .segments
            .iter()
            .find(|s| s.id == id)
            .context("Passage introuvable")?;
        let (a, b) = (seg.start_ms, seg.end_ms);
        let source = self.audio_dir(self.id).join(&seg.track);
        let dlg = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .add_filter("Extrait WAV", &["wav"])
            .set_file_name(format!("enregistrement-{}-passage-{id}.wav", self.id));
        cx.spawn(async move |desk, cx| {
            if let Some(file) = dlg.save_file().await {
                let result = cx
                    .background_executor()
                    .spawn(async move { audio_capture::export_segment(&source, file.path(), a, b) })
                    .await;
                let _ = desk.update(cx, |s, cx| {
                    s.message = match result {
                        Ok(()) => "Extrait WAV enregistré.".into(),
                        Err(e) => format!("{e:#}"),
                    };
                    cx.notify();
                });
            }
        })
        .detach();
        Ok(())
    }
    fn busy(&self) -> bool {
        self.ctl.is_some() || self.cancel.is_some()
    }
    fn refresh(&mut self, q: &str) -> Result<()> {
        self.library = self.db.list(q)?;
        Ok(())
    }
    fn list_sources(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.last_scan = Instant::now();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Event::Sources(discover()));
        });
    }
    fn detail(&mut self, id: i64) -> Result<()> {
        self.stop_segment();
        self.metadata = false;
        self.id = id;
        self.suggestions = self.db.suggestions(id)?;
        self.segments = self.db.segments(id)?;
        self.translations = self.db.translations(id)?;
        self.language_picker = None;
        self.dialogues
            .reset_with_uniform_height(self.segments.len(), px(99.));
        self.summary_ids = self.db.summary_ids(id)?;
        self.transcript_ids = self.db.transcript_versions(id)?;
        self.reading_version = [
            self.summary_ids.first().copied(),
            self.transcript_ids.first().copied(),
        ];
        self.set_report(self.db.summary(id)?.unwrap_or_default());
        self.record_menu = None;
        self.scroll_grab = None;
        self.edit_id = None;
        self.page = Page::Detail;
        self.refresh("")
    }
    fn set_report(&mut self, text: String) {
        self.report_lines = text.lines().map(str::to_owned).collect();
        self.report_list
            .reset_with_uniform_height(self.report_lines.len(), px(22.));
        self.summary = text;
    }
    fn save_settings(&self) -> Result<()> {
        self.db.set_pref("settings", &self.settings.to_string())
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.page == Page::Settings
            && self
                .last_measure
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
            && self.monitor.try_send(()).is_ok()
        {
            self.last_measure = Some(Instant::now());
        }

        if let Some(p) = &mut self.playing
            && !p.paused
        {
            match super::playback::is_playing()
                .and_then(|playing| Ok((playing, super::playback::position()?)))
            {
                Ok((true, position)) if position < p.end => p.position = position,
                Ok(_) => self.stop_segment(),
                Err(e) => {
                    self.stop_segment();
                    self.message = e.to_string();
                }
            }
            cx.notify();
        }
        while let Ok(event) = self.rx.try_recv() {
            if let Err(e) = self.apply(event, cx) {
                self.message = format!("{e:#}");
            }
            cx.notify();
        }
        if let Some(button) = &mut self.taskbar {
            button.sync();
        }
        if self.update_status == "applying" {
            cx.quit();
            return;
        }
        if self.busy() || self.preview.is_some() {
            cx.notify();
        }
        if self.page == Page::New
            && !self.busy()
            && self.preview.is_none()
            && self.last_scan.elapsed() > Duration::from_secs(2)
        {
            self.list_sources();
        }
    }
    fn apply(&mut self, event: Event, cx: &App) -> Result<()> {
        match event {
            Event::Sources(result) => {
                self.scanning = false;
                if self.preview.is_some() {
                    return Ok(());
                }
                self.inputs = result?;
                self.chosen
                    .retain(|src| self.inputs.iter().any(|i| &i.source == src));
                if self.initial {
                    self.initial = false;
                    let last: Vec<String> = self
                        .db
                        .pref("selection")?
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();
                    for i in &self.inputs {
                        let key = pref_key(&i.source);
                        if last.contains(&key)
                            && self
                                .inputs
                                .iter()
                                .filter(|other| pref_key(&other.source) == key)
                                .count()
                                == 1
                        {
                            self.chosen.insert(i.source.clone());
                        }
                    }
                }
            }
            Event::Translated(id, original, lang, r) => {
                self.cancel = None;
                let value = r?;
                let text = value["text"].as_str().context("La traduction est vide")?;
                self.db.store_translation(id, &original, &lang, text)?;
                self.translations = self.db.translations(self.id)?;
                if let Some(i) = self.segments.iter().position(|s| s.id == id) {
                    self.dialogues.remeasure_items(i..i + 1);
                }
            }
            Event::Update(op, res) => self.finish_update(op, res)?,
            Event::StorageChanged(result) => {
                self.cancel = None;
                self.page = Page::Settings;
                let result = result?;
                self.apply_inventory(result["installed"].clone(), result["storage"].clone())?;
                self.message = if result["storage"]["cleanup_pending"] == true {
                    "Emplacement modifié. Certains originaux sont encore utilisés."
                } else {
                    "Emplacement des composants enregistré."
                }
                .into();
            }
            Event::Hardware(s, installed, ready, storage) => {
                self.apply_inventory(installed, storage)?;
                self.nvidia_ready = ready;
                if s == "GPU NVIDIA détecté" {
                    self.recommended = "Précision Turbo".into();
                }
                self.hardware.push_str(&format!(" · {s}"));
            }
            Event::Load(v) => {
                self.load = v;
            }
            Event::GpuTest(r) => {
                self.cancel = None;
                let v = r.context(
                    "Test CUDA en échec · vérifiez le pilote NVIDIA et le moteur installé",
                )?;
                self.message = format!(
                    "Test réussi sur CUDA · encodeur et décodeur en {}.",
                    v["compute_type"].as_str().unwrap_or("float16")
                );
            }
            Event::Recovered(id, result) => {
                self.cancel = None;
                let (count, total) = result?;
                self.db.status(id, "Pistes récupérées")?;
                self.detail(id)?;
                self.message = format!(
                    "{count} piste(s) lisible(s) sur {total}. Les originaux sont conservés."
                );
            }
            Event::Installed(r) => {
                self.finish_download();
                self.cancel = None;
                tracing::info!(reussite = r.is_ok(), "Fin du téléchargement du modèle");
                let v = match r {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(e);
                    }
                };
                self.page = Page::Settings;
                if v["runtime"].is_string() {
                    self.nvidia_ready = true;
                    self.settings["device"] = json!("cuda");
                    self.settings["strict_cuda"] = json!(true);
                    self.save_settings()?;
                    self.message =
                        "Moteur NVIDIA prêt. Vérifiez CUDA avec le modèle choisi.".into();
                    return Ok(());
                }
                if let Some(kind) = v["kind"].as_str() {
                    let (key, preset) = if kind == "summary" {
                        ("gguf", v["preset"].as_str().unwrap_or("Résumé local"))
                    } else {
                        ("translation", "Traduction locale")
                    };
                    self.settings[key] = v["path"].clone();
                    self.installed[preset] = v.clone();
                    self.save_settings()?;
                    if let Some((id, lang)) = self.pending_translation.take() {
                        self.page = Page::Detail;
                        self.start_translation(id, lang)?;
                    } else if std::mem::take(&mut self.pending_summary) {
                        self.page = Page::Detail;
                        self.process(true)?;
                    }
                    return Ok(());
                }
                if let Some(preset) = Page::MODEL_PRESETS
                    .iter()
                    .find(|preset| Self::repo_for(preset) == v["repo"].as_str().unwrap_or(""))
                {
                    self.installed[*preset] = v.clone();
                }
                self.settings["whisper"] = v["folder"].clone();
                self.settings["whisper_revision"] = v["revision"].clone();
                self.settings["whisper_repo"] = v["repo"].clone();
                self.save_settings()?;
                self.message = "Modèle vérifié et sélectionné.".into();
            }
            Event::Enrolled(id, label, v) => {
                self.cancel = None;
                let profile = self.db.add_profile(&v?)?;
                self.db.confirm(id, &label, Some(profile))?;
                self.detail(id)?;
            }
            Event::Stage(update) => {
                if let (Some(button), Some((done, total))) = (&mut self.taskbar, update.bytes) {
                    button.progress(done, total);
                }
                if self.stage != update.stage {
                    tracing::info!(etape = %update.stage, "Progression IA");
                }
                self.stage = update.stage;
                self.transfer = update.bytes;
                self.generated_chars = update.generated_chars;
                self.last_progress = Instant::now();
            }
            Event::LevelsTested(result) => {
                let start_after = self
                    .preview
                    .take()
                    .is_some_and(|preview| preview.start_after);
                if self.page == Page::New {
                    result.context("Test des niveaux")?;
                    if start_after {
                        self.start(cx)?;
                    }
                }
            }
            Event::Recorded(id, res) => {
                self.confirmation = None;
                let duration = self.ctl.take().map(|c| c.elapsed_ms()).unwrap_or(0);
                self.stopping = false;
                if self.discard {
                    self.discard = false;
                    self.delete(id, true)?;
                    self.page = Page::Library;
                    self.refresh("")?;
                    return Ok(());
                }
                tracing::info!(meeting = id, reussite = res.is_ok(), "Fin de capture");
                self.db.finish(
                    id,
                    duration as i64,
                    if res.is_ok() {
                        "Audio finalisé"
                    } else {
                        "Capture interrompue"
                    },
                )?;
                self.detail(id)?;
                let manifest = res?;
                for t in manifest.tracks {
                    self.db.track(
                        id,
                        &t.input.name,
                        &serde_json::to_string(&t.input.source)?,
                        &t.file,
                    )?;
                }
                if self.settings["whisper"].is_string() {
                    self.process(false)?;
                } else {
                    self.message =
                        "Audio enregistré. Choisissez un modèle de transcription dans Paramètres."
                            .into();
                }
            }
            Event::Processed(id, summary, mut settings, res) => {
                let partial = res.as_ref().is_ok_and(|v| v["partial"] == true);
                self.cancel = None;
                let saved: Result<()> = (|| {
                    let result = res?;
                    if summary {
                        self.db.save_summary(
                            id,
                            result["body"].as_str().context("Synthèse manquante")?,
                            &settings.to_string(),
                            &serde_json::from_value::<Vec<i64>>(result["evidence"].clone())?,
                        )?;
                        self.db.status(id, "Terminée")?;
                    } else {
                        if let Some(device) = result["device"].as_str() {
                            settings["transcription_device"] = json!(device);
                        }
                        self.db.publish(
                            id,
                            &settings.to_string(),
                            &serde_json::from_value::<Vec<Segment>>(result["segments"].clone())?,
                            &serde_json::from_value::<Vec<_>>(result["turns"].clone())?,
                            serde_json::from_value(result["proposals"].clone())?,
                        )?;
                    }
                    Ok(())
                })();
                if let Some(job) = self.job.take() {
                    self.db
                        .finish_job(job, saved.as_ref().err().map(|e| e.to_string()).as_deref())?;
                }
                if saved.is_err() {
                    self.db.status(id, "Traitement interrompu")?;
                }
                self.detail(id)?;
                self.tab = if summary { 0 } else { 1 };
                tracing::info!(
                    reussite = saved.is_ok(),
                    duree_s = self.work_started.elapsed().as_secs(),
                    "Fin du traitement IA"
                );
                saved.with_context(|| format!("Traitement interrompu · {}", self.stage))?;
                self.message = if summary && partial {
                    "Compte rendu partiel enregistré. Certains passages restent à vérifier.".into()
                } else if summary {
                    "Compte rendu enregistré.".into()
                } else if self.segments.is_empty() {
                    "Transcription terminée. Aucune parole détectée sur les pistes.".into()
                } else {
                    format!(
                        "Transcription terminée · {} segment{}.",
                        self.segments.len(),
                        if self.segments.len() > 1 { "s" } else { "" }
                    )
                };
                if !summary && settings["transcription_device"] == "cuda" {
                    self.message.push_str(" Calcul sur GPU NVIDIA (CUDA).");
                }
                if !summary
                    && settings["device"] == "cuda"
                    && settings["transcription_device"] == "cpu"
                {
                    self.message
                        .push_str(" Calcul sur CPU (CUDA indisponible).");
                }
                if !summary && settings["auto_summary"] == true && !self.segments.is_empty() {
                    self.process(true)?;
                }
            }
        }
        Ok(())
    }
    fn selected_inputs(&self) -> Vec<Input> {
        self.inputs
            .iter()
            .filter(|i| self.chosen.contains(&i.source))
            .cloned()
            .collect()
    }
    fn gain_db(&self, source: &Source) -> i8 {
        self.gains.get(source).copied().unwrap_or_else(|| {
            self.settings["input_gains_db"][pref_key(source)]
                .as_i64()
                .unwrap_or(0)
                .clamp(
                    i64::from(audio_capture::MIN_GAIN_DB),
                    i64::from(audio_capture::MAX_GAIN_DB),
                ) as i8
        })
    }
    fn set_gain(&mut self, source: &Source, db: i8) -> Result<()> {
        let db = db.clamp(audio_capture::MIN_GAIN_DB, audio_capture::MAX_GAIN_DB);
        if self.gain_db(source) == db {
            return Ok(());
        }
        self.gains.insert(source.clone(), db);
        if let Some(preview) = &self.preview {
            preview.ctl.set_gain_db(source, db);
        }
        let key = pref_key(source);
        if self
            .inputs
            .iter()
            .filter(|i| pref_key(&i.source) == key)
            .count()
            == 1
        {
            self.settings["input_gains_db"][key] = json!(db);
            self.save_settings()?;
        }
        Ok(())
    }
    fn audio_control(&self, inputs: &[Input]) -> Control {
        let ctl = Control::default();
        for input in inputs {
            ctl.set_gain_db(&input.source, self.gain_db(&input.source));
        }
        ctl
    }
    fn test_levels(&mut self) -> Result<()> {
        ensure!(
            self.preview.is_none() && !self.busy(),
            "Un flux audio est déjà actif."
        );
        let inputs = self.selected_inputs();
        ensure!(
            !inputs.is_empty(),
            "Sélectionnez une application ou un microphone à tester."
        );
        let ctl = self.audio_control(&inputs);
        self.preview = Some(LevelTest {
            ctl: ctl.clone(),
            tracks: inputs.iter().map(|i| i.source.clone()).collect(),
            stopping: false,
            start_after: false,
        });
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Event::LevelsTested(audio_capture::test_levels(inputs, ctl)));
        });
        Ok(())
    }
    fn stop_test(&mut self, start_after: bool) {
        if let Some(test) = &mut self.preview {
            test.start_after = start_after;
            test.stopping = true;
            test.ctl.stop();
        }
    }
    fn start(&mut self, cx: &App) -> Result<()> {
        ensure!(!self.busy(), "Un enregistrement est déjà en cours");
        self.stop_segment();
        let inputs = self.selected_inputs();
        ensure!(
            !inputs.is_empty(),
            "Sélectionnez une application ou un microphone."
        );
        if self.settings["diarization"] == true {
            ensure!(
                self.settings["segmentation"].is_string() && self.settings["embedding"].is_string(),
                "Choisissez les deux modèles Sherpa dans Paramètres."
            );
        }
        let title = self.title.read(cx).as_str().trim();
        ensure!(!title.is_empty(), "Donnez un titre à l’enregistrement.");
        if self.settings["auto_summary"] == true {
            ensure!(
                self.settings["gguf"].is_string() && self.settings["whisper"].is_string(),
                "Sélectionnez un modèle GGUF et un modèle Faster-Whisper pour le résumé automatique."
            );
        }
        if self.preview.is_some() {
            self.stop_test(true);
            return Ok(());
        }
        self.db.set_pref(
            "selection",
            &serde_json::to_string(
                &inputs
                    .iter()
                    .map(|i| pref_key(&i.source))
                    .collect::<Vec<_>>(),
            )?,
        )?;
        self.id = self.db.create(
            title,
            self.project.read(cx).as_str(),
            &self.settings.to_string(),
        )?;
        self.capture_names = inputs.iter().map(|i| i.name.clone()).collect();
        let ctl = self.audio_control(&inputs);
        self.ctl = Some(ctl.clone());
        self.paused = false;
        self.stopping = false;
        let id = self.id;
        let dir = self.root.join("audio").join(id.to_string());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let r = record(&dir, inputs, ctl);
            let _ = tx.send(Event::Recorded(id, r));
        });
        Ok(())
    }
    fn audio_dir(&self, id: i64) -> PathBuf {
        let root = self.root.join("audio").join(id.to_string());
        if root.join("reprise/finalise").is_file() {
            root.join("reprise")
        } else {
            root
        }
    }
    fn begin_work(&mut self) -> Arc<AtomicBool> {
        let ctl = Arc::new(AtomicBool::new(false));
        self.cancel = Some(ctl.clone());
        self.work_started = Instant::now();
        self.last_progress = self.work_started;
        self.transfer = None;
        self.generated_chars = None;
        ctl
    }
    fn begin_download(&mut self) -> Arc<AtomicBool> {
        if let Some(button) = &mut self.taskbar {
            button.start();
        }
        self.begin_work()
    }
    fn finish_download(&mut self) {
        if let Some(button) = &mut self.taskbar {
            button.finish();
        }
    }
    fn recover_audio(&mut self) -> Result<()> {
        let id = self.id;
        let dir = self.audio_dir(id);
        let ctl = self.begin_work();
        self.stage = "Récupération des fragments audio".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let res = audio_capture::recover_tracks(&dir, &ctl);
            let _ = tx.send(Event::Recovered(id, res));
        });
        Ok(())
    }
    fn ask(
        &mut self,
        title: &'static str,
        detail: impl Into<String>,
        accept: &'static str,
        action: impl FnOnce(&mut Self) -> Result<()> + 'static,
    ) -> Result<()> {
        self.confirmation = Some(Confirmation {
            title,
            detail: detail.into(),
            accept,
            action: Box::new(action),
        });
        Ok(())
    }
    fn request_remote_summary(&mut self) -> Result<()> {
        let url = self.settings["endpoint"]
            .as_str()
            .context("Configurez le fournisseur dans Paramètres.")?;
        self.ask("Envoyer la transcription ?", format!("Le texte et les noms des intervenants seront envoyés à {url} pour générer le résumé."), "Envoyer", |s|s.process_via(true,true))
    }
    fn process(&mut self, summary: bool) -> Result<()> {
        self.process_via(summary, false)
    }
    fn process_via(&mut self, summary: bool, remote: bool) -> Result<()> {
        ensure!(!self.busy(), "Un travail est déjà actif");
        let id = self.id;
        let dir = self.audio_dir(id);
        ensure!(
            summary || dir.join("finalise").is_file(),
            "Audio incomplet. Vérifiez les pistes récupérées."
        );
        let settings = self.settings.clone();
        let params = if summary {
            if remote {
                ensure!(
                    settings["endpoint"].is_string() && settings["remote_model"].is_string(),
                    "Configurez le fournisseur dans Paramètres."
                );
            }
            ensure!(
                !self.segments.is_empty() && (remote || settings["gguf"].is_string()),
                "Choisissez un modèle GGUF et un enregistrement transcrit."
            );
            json!({"settings":settings,"remote_consent":remote,"segments":self.db.segments(id)?})
        } else {
            ensure!(
                settings["whisper"].is_string(),
                "Choisissez le dossier Faster-Whisper dans Paramètres."
            );
            json!({"settings":settings,"directory":dir,"profiles":if settings["recognize"]==true{self.db.profiles(true)?}else{vec![]}})
        };
        self.job = Some(
            self.db
                .start_job(id, if summary { "Résumé" } else { "Transcription" })?,
        );
        let ctl = self.begin_work();
        let tx = self.tx.clone();
        self.stage = "Ouverture du moteur IA…".into();
        thread::spawn(move || {
            let res = super::worker::run(
                id,
                if summary { "summarize" } else { "transcribe" },
                params,
                ctl,
                |s| {
                    let _ = tx.send(Event::Stage(s));
                },
            );
            let _ = tx.send(Event::Processed(id, summary, settings, res));
        });
        Ok(())
    }
    fn go(&mut self, page: Page) -> Result<()> {
        self.record_menu = None;
        self.language_picker = None;
        if self.busy() {
            return Ok(());
        }
        self.stop_test(false);
        self.menu = None;
        self.stop_segment();
        if page == Page::Settings {
            self.settings_origin = self.page;
        } else {
            self.hits.clear();
        }
        self.page = page;
        self.message.clear();
        if page == Page::New {
            self.list_sources();
        }
        if page == Page::Profiles {
            self.profiles = self.db.profiles(false)?;
        }
        if page == Page::Home {
            self.excerpts = self.db.excerpts(&self.browse_term)?;
        }
        if page == Page::Home || page == Page::Library {
            self.refresh("")?;
        }
        Ok(())
    }
    fn check_cuda(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Un traitement est en cours");
        ensure!(
            self.settings["whisper"].is_string(),
            "Choisissez d’abord un modèle de transcription."
        );
        let ctl = self.begin_work();
        let tx = self.tx.clone();
        let params = json!({"settings": self.settings});
        self.stage = "Vérification CUDA".into();
        thread::spawn(move || {
            let result = super::worker::run(0, "test_cuda", params, ctl, |s| {
                let _ = tx.send(Event::Stage(s));
            });
            let _ = tx.send(Event::GpuTest(result));
        });
        Ok(())
    }
    fn answer_download(&mut self, approved: bool) -> Result<()> {
        if self.busy() {
            return Ok(());
        }
        if !approved && (self.pending_translation.is_some() || self.pending_summary) {
            self.pending_translation = None;
            self.pending_summary = false;
            self.page = self.settings_origin;
            return Ok(());
        }
        let Some(preset) = self.page.answer_download(approved) else {
            return Ok(());
        };
        ensure!(!self.busy(), "Un travail est déjà actif");
        tracing::info!(preset, "Téléchargement du modèle confirmé");
        let ctl = self.begin_download();
        self.stage = "Mise en relation avec le catalogue public".into();
        let params = json!({"consent":true,"preset":preset});
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let method = if preset == "NVIDIA CUDA" {
                "install_cuda"
            } else {
                "install"
            };
            let r = super::worker::run(0, method, params, ctl, |s| {
                let _ = tx.send(Event::Stage(s));
            });
            let _ = tx.send(Event::Installed(r));
        });
        Ok(())
    }
    fn pick(
        &mut self,
        key: &'static str,
        folder: bool,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let dlg = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .set_title(tr("Sélectionner un modèle local"));
        cx.spawn(async move |desk, cx| {
            let path = if folder {
                dlg.pick_folder().await
            } else {
                dlg.add_filter("Modèle", &[if key == "gguf" { "gguf" } else { "onnx" }])
                    .pick_file()
                    .await
            };
            if let Some(path) = path {
                let _ = desk.update(cx, |s, cx| {
                    s.settings[key] = json!(path.path());
                    if key == "whisper" {
                        s.settings["whisper_revision"] = Value::Null;
                        s.settings["whisper_repo"] = Value::Null;
                    }
                    if let Err(e) = s.save_settings() {
                        s.message = e.to_string();
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        Ok(())
    }
    fn delete(&mut self, id: i64, all: bool) -> Result<()> {
        self.stop_segment();
        let dir = self.root.join("audio").join(id.to_string());
        if dir.is_dir() {
            fs::remove_dir_all(dir)?;
        }
        if all {
            self.db.delete_meeting(id)?;
        }
        Ok(())
    }
    fn search_db(&mut self, cx: &App) -> Result<()> {
        let q = self.search.read(cx).as_str();
        self.refresh(q)?;
        self.db.filter(
            &mut self.library,
            self.filter.read(cx).as_str(),
            self.day.read(cx).as_str(),
        )?;
        self.hits = self
            .db
            .hits(q)?
            .into_iter()
            .filter(|h| self.library.iter().any(|m| m.id == h.meeting))
            .collect();
        Ok(())
    }
    fn enroll(&mut self, seg: Segment, name: String) -> Result<()> {
        ensure!(
            self.settings["embedding"].is_string(),
            "Choisissez un modèle d’empreintes ONNX dans Paramètres."
        );
        ensure!(!name.trim().is_empty(), "Indiquez le nom du profil.");
        self.ask("Créer un profil vocal ?",format!("La personne autorise-t-elle la conservation locale de cet extrait de voix et de son empreinte sous le nom « {name} » ?"),"Créer le profil",move |s| s.enroll_confirmed(seg,name))
    }
    fn enroll_confirmed(&mut self, seg: Segment, name: String) -> Result<()> {
        let id = self.id;
        let label = seg.acoustic_label.clone();
        let file = self.audio_dir(id).join(&seg.track);
        let params =
            json!({"consent":true,"segment":seg,"name":name,"file":file,"settings":self.settings});
        let ctl = self.begin_work();
        self.stage = "Création du profil vocal".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let res = super::worker::run(id, "enroll", params, ctl, |s| {
                let _ = tx.send(Event::Stage(s));
            });
            let _ = tx.send(Event::Enrolled(id, label, res));
        });
        Ok(())
    }
    fn export(&self, fmt: &'static str, w: &Window, cx: &mut Context<Self>) -> Result<()> {
        let dlg = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .add_filter(fmt, &[fmt])
            .set_file_name(format!("enregistrement-{}.{}", self.id, fmt));
        let meeting = self
            .library
            .iter()
            .find(|m| m.id == self.id)
            .context("Enregistrement introuvable")?;
        let segments = if self.summary_only {
            vec![]
        } else {
            self.segments.clone()
        };
        let text = match fmt {
            "json" => serde_json::to_string_pretty(
                &json!({"meeting":meeting,"summary":self.summary,"segments":segments}),
            )?,
            _ => {
                let mut text = format!(
                    "{}{}\n\n{}\n\n",
                    if fmt == "md" { "# " } else { "" },
                    meeting.title,
                    self.summary
                );
                for s in &segments {
                    text.push_str(&format!(
                        "[{} · §{}] {} — {}\n",
                        time(s.start_ms as u64),
                        s.id,
                        s.speaker,
                        s.text
                    ));
                }
                text
            }
        };
        cx.spawn(async move |desk, cx| {
            if let Some(file) = dlg.save_file().await {
                let result = cx
                    .background_executor()
                    .spawn(async move { fs::write(file.path(), text) })
                    .await;
                let _ = desk.update(cx, |s, cx| {
                    s.message = match result {
                        Ok(()) => "Export enregistré.".into(),
                        Err(e) => e.to_string(),
                    };
                    cx.notify();
                });
            }
        })
        .detach();
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordMenu {
    Export,
    More,
    History,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inspector {
    Session,
    Audio,
    Models,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preference {
    Theme,
    Device,
    Language,
    Locale,
}
impl Drop for Desk {
    fn drop(&mut self) {
        self.stop_test(false);
        super::playback::stop();
    }
}

fn pref_key(s: &Source) -> String {
    match s {
        Source::Application { exe, .. } => format!("app:{}", exe.display()),
        Source::Microphone { endpoint } => format!("mic:{endpoint}"),
    }
}
fn time(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

#[path = "view.rs"]
mod view;
