use anyhow::{Context as _, Result, ensure};
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
    collections::HashSet,
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
    let root =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("Profil Windows introuvable")?)
            .join("MeetingNotes");
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
    tracing::info!(build, "Ouverture de la bibliothèque");
    let db = Library::open(&root.join("reunions.sqlite3"))?;
    db.recover()?;
    gpui_platform::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx| {
            let _ = cx
                .text_system()
                .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                    "../../../assets/inter/Inter.ttf"
                ))]);
            cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
            let bounds = Bounds::centered(None, size(px(990.), px(725.)), cx);
            let opt = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("MeetingNotes".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            cx.open_window(opt, move |w, cx| {
                let desk = cx.new(move |cx| Desk::new(root, db, cx));
                let weak = desk.downgrade();
                w.on_window_should_close(cx, move |_, cx| {
                    weak.update(cx, |s, cx| {
                        if s.busy() {
                            s.message =
                                "Arrêtez la capture ou annulez le traitement avant de fermer."
                                    .into();
                            cx.notify();
                            false
                        } else {
                            true
                        }
                    })
                    .unwrap_or(true)
                });
                desk
            })
            .expect("Ouverture de la fenêtre MeetingNotes");
            cx.activate(true);
        });
    drop(lock);
    Ok(())
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    New,
    Library,
    Detail,
    Profiles,
    Settings,
}
enum Event {
    Hardware(String),
    Recovered(i64, Result<(usize, usize)>),
    Installed(Result<Value>),
    Enrolled(i64, String, Result<Value>),
    Sources(Result<Vec<Input>>),
    Recorded(i64, Result<Manifest>),
    Stage(String),
    Processed(i64, bool, Value, Result<Value>),
}
type Field = Entity<EditableTextState>;
fn field(cx: &mut Context<Desk>) -> Field {
    cx.new(|cx| EditableTextState::new(StringStorage::default(), cx))
}
struct Desk {
    hardware: String,
    recommended: String,
    root: PathBuf,
    db: Library,
    page: Page,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    inputs: Vec<Input>,
    chosen: HashSet<Source>,
    scanning: bool,
    last_scan: Instant,
    initial: bool,
    ctl: Option<Control>,
    capture_names: Vec<String>,
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
    limit: usize,
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
    fn new(root: PathBuf, db: Library, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = mpsc::channel();
        let settings = db
            .pref("settings")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({"language":"fr","device":"cpu","self_name":"Moi"}));
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
            let gpu = match res {
                Ok(v) if v["cuda"].as_u64().unwrap_or(0) > 0 => "CUDA détecté",
                Ok(_) => "Calcul sur processeur",
                Err(_) => "Worker indisponible",
            };
            let _ = tx_hw.send(Event::Hardware(gpu.into()));
        });
        let mut s = Self {
            hardware,
            recommended,
            root,
            db,
            page: Page::Home,
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
            limit: 80,
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
            (&s.self_name, "self_name"),
            (&s.endpoint, "endpoint"),
            (&s.remote_model, "remote_model"),
        ] {
            if let Some(v) = s.settings[k].as_str() {
                f.update(cx, |f, cx| f.emplace(v, cx));
            }
        }
        if let Err(e) = s.refresh("") {
            s.message = e.to_string();
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this.update(cx, |s, cx| s.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        s
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
        self.limit = 80;
        self.metadata = false;
        self.id = id;
        self.suggestions = self.db.suggestions(id)?;
        self.segments = self.db.segments(id)?;
        self.summary = self.db.summary(id)?.unwrap_or_default();
        self.edit_id = None;
        self.page = Page::Detail;
        self.refresh("")
    }
    fn save_settings(&self) -> Result<()> {
        self.db.set_pref("settings", &self.settings.to_string())
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        while let Ok(event) = self.rx.try_recv() {
            if let Err(e) = self.apply(event) {
                self.message = format!("{e:#}");
            }
            cx.notify();
        }
        if self.ctl.is_some() {
            cx.notify();
        }
        if self.page == Page::New
            && !self.busy()
            && self.last_scan.elapsed() > Duration::from_secs(2)
        {
            self.list_sources();
        }
    }
    fn apply(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Sources(result) => {
                self.scanning = false;
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
            Event::Hardware(s) => self.hardware.push_str(&format!(" · {s}")),
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
                self.cancel = None;
                let v = r?;
                self.settings["whisper"] = v["folder"].clone();
                self.settings["whisper_revision"] = v["revision"].clone();
                self.save_settings()?;
                self.message = "Modèle vérifié et sélectionné.".into();
            }
            Event::Enrolled(id, label, v) => {
                self.cancel = None;
                let profile = self.db.add_profile(&v?)?;
                self.db.confirm(id, &label, Some(profile))?;
                self.detail(id)?;
            }
            Event::Stage(s) => self.stage = s,
            Event::Recorded(id, res) => {
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
            Event::Processed(id, summary, settings, res) => {
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
                saved?;
                self.tab = if summary { 0 } else { 1 };
                if !summary && settings["auto_summary"] == true && !self.segments.is_empty() {
                    self.process(true)?;
                }
            }
        }
        Ok(())
    }
    fn start(&mut self, cx: &App) -> Result<()> {
        ensure!(!self.busy(), "Une réunion est déjà active");
        super::playback::stop();
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .filter(|i| self.chosen.contains(&i.source))
            .cloned()
            .collect();
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
        ensure!(!title.is_empty(), "Donnez un titre à la réunion.");
        if self.settings["auto_summary"] == true {
            ensure!(
                self.settings["gguf"].is_string() && self.settings["whisper"].is_string(),
                "Sélectionnez un modèle GGUF et un modèle Faster-Whisper pour le résumé automatique."
            );
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
        let ctl = Control::default();
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
    fn recover_audio(&mut self) -> Result<()> {
        let id = self.id;
        let dir = self.audio_dir(id);
        let ctl = Arc::new(AtomicBool::new(false));
        self.cancel = Some(ctl.clone());
        self.stage = "Récupération des fragments audio".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let res = audio_capture::recover_tracks(&dir, &ctl);
            let _ = tx.send(Event::Recovered(id, res));
        });
        Ok(())
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
                if !view::confirm(&format!(
                    "Envoyer le texte et les noms des intervenants de cette réunion à {} pour générer un résumé ?",
                    settings["endpoint"].as_str().unwrap_or_default()
                )) {
                    return Ok(());
                }
            }
            ensure!(
                !self.segments.is_empty() && (remote || settings["gguf"].is_string()),
                "Choisissez un modèle GGUF et une réunion transcrite."
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
        let ctl = Arc::new(AtomicBool::new(false));
        self.cancel = Some(ctl.clone());
        let tx = self.tx.clone();
        self.stage = "Démarrage du worker IA".into();
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
        super::playback::stop();
        self.hits.clear();
        if self.busy() {
            return Ok(());
        }
        self.page = page;
        self.message.clear();
        if page == Page::New {
            self.list_sources();
        }
        if page == Page::Profiles {
            self.profiles = self.db.profiles(false)?;
        }
        if page == Page::Home || page == Page::Library {
            self.refresh("")?;
        }
        Ok(())
    }
    fn install(&mut self, preset: &str) -> Result<()> {
        if !view::confirm(&format!(
            "Télécharger le modèle « {preset} » depuis Hugging Face et le conserver sur cet ordinateur ?"
        )) {
            return Ok(());
        }
        let ctl = Arc::new(AtomicBool::new(false));
        self.cancel = Some(ctl.clone());
        self.stage = "Mise en relation avec le catalogue public".into();
        let params = json!({"consent":true,"cache":self.root.join("modeles"),"preset":preset});
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = super::worker::run(0, "install", params, ctl, |s| {
                let _ = tx.send(Event::Stage(s));
            });
            let _ = tx.send(Event::Installed(r));
        });
        Ok(())
    }
    fn pick(&mut self, key: &str, folder: bool) -> Result<()> {
        let dlg = rfd::FileDialog::new().set_title("Sélectionner un modèle local");
        let path = if folder {
            dlg.pick_folder()
        } else {
            dlg.add_filter("Modèle", &[if key == "gguf" { "gguf" } else { "onnx" }])
                .pick_file()
        };
        if let Some(p) = path {
            self.settings[key] = json!(p);
            self.save_settings()?;
        }
        Ok(())
    }
    fn delete(&mut self, id: i64, all: bool) -> Result<()> {
        super::playback::stop();
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
        let id = self.id;
        let label = seg.acoustic_label.clone();
        let file = self.audio_dir(id).join(&seg.track);
        ensure!(rfd::MessageDialog::new().set_title("Créer un profil vocal").set_description(format!("La personne autorise-t-elle la conservation locale de cet extrait de sa voix et de son empreinte sous le nom « {name} » ?")).set_buttons(rfd::MessageButtons::YesNo).show()==rfd::MessageDialogResult::Yes,"Création annulée");
        let params =
            json!({"consent":true,"segment":seg,"name":name,"file":file,"settings":self.settings});
        let ctl = Arc::new(AtomicBool::new(false));
        self.cancel = Some(ctl.clone());
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
    fn export(&self, fmt: &str) -> Result<()> {
        let Some(p) = rfd::FileDialog::new()
            .add_filter(fmt, &[fmt])
            .set_file_name(format!("reunion-{}.{}", self.id, fmt))
            .save_file()
        else {
            return Ok(());
        };
        let meeting = self
            .library
            .iter()
            .find(|m| m.id == self.id)
            .context("Réunion absente")?;
        let segments = if self.summary_only {
            vec![]
        } else {
            self.db.segments(self.id)?
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
        fs::write(p, text)?;
        Ok(())
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
