use super::*;
use unicode_segmentation::UnicodeSegmentation;

enum LiveEvent {
    Sources(Result<Vec<Input>>),
    Capture(Control, Instant),
    Progress(crate::worker::Progress),
    Finished(Result<()>),
}

struct Phrase {
    key: (u64, usize, u64),
    confirmed: bool,
    animate_translation: bool,
    speaker: String,
    original: String,
    translated: Option<(String, Instant)>,
    appeared: Instant,
    previous_prefix: usize,
}

pub(in crate::ui) struct Live {
    owner: WindowHandle<Desk>,
    inputs: Vec<Input>,
    selected: HashSet<Source>,
    target: String,
    identify: bool,
    recognize: bool,
    search: Field,
    picker: bool,
    settings_open: bool,
    settings_focus: FocusHandle,
    section: usize,
    sidebar: Sidebar,
    settings_scroll: ScrollHandle,
    source_menu: Option<SourceMenu>,
    scanning: bool,
    status: String,
    control: Option<Control>,
    capture_start: Option<Instant>,
    latency: Option<Duration>,
    cancel: Option<Arc<AtomicBool>>,
    stopping: bool,
    closing: bool,
    names: Vec<String>,
    phrases: std::collections::VecDeque<Phrase>,
    scroll: ScrollHandle,
    tx: mpsc::Sender<LiveEvent>,
    rx: mpsc::Receiver<LiveEvent>,
}

fn speaker_label(source: Option<&String>, v: &Value) -> String {
    let source = source.map(String::as_str).unwrap_or("");
    let label = match &v["speaker"] {
        id if id.is_u64() => Some(format!("{} {}", tr("Intervenant"), id)),
        s if s == "uncertain" => Some(tr("Intervenant incertain").to_owned()),
        s if s == "overlap" => Some(tr("Voix superposées").to_owned()),
        _ => None,
    };
    let mut title = label.map_or_else(
        || source.to_owned(),
        |name| {
            [source, &name]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        },
    );
    if let Some(name) = v["suggestion"].as_str() {
        title.push_str(&format!(" · {name} ?"));
    }
    title
}

impl Desk {
    pub(super) fn open_live(&mut self, w: &Window, cx: &mut Context<Self>) -> Result<()> {
        if self
            .live_window
            .is_some_and(|handle| handle.update(cx, |_, w, _| w.activate()).is_ok())
        {
            return Ok(());
        }
        let owner = w
            .window_handle()
            .downcast::<Desk>()
            .context("Fenêtre Rikai introuvable")?;
        let selected = self.chosen.clone();
        let identify = self.settings["diarization"] == true;
        let recognize = self.settings["recognize"] == true;
        let opt = window_frame::options(
            Live::window_title(),
            size(px(800.), px(580.)),
            size(px(480.), px(340.)),
            cx,
        );
        self.live_window = Some(cx.open_window(opt, |w, cx| {
            let view = cx.new(|cx| Live::new(owner, selected, identify, recognize, cx));
            let weak = view.downgrade();
            w.on_window_should_close(cx, move |_, cx| {
                weak.update(cx, Live::can_close).unwrap_or(true)
            });
            view
        })?);
        Ok(())
    }
}

impl Live {
    pub(super) fn window_title() -> String {
        format!("Rikai · {}", tr("Traduction"))
    }

    fn can_close(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(cancel) = self.cancel.clone() else {
            return true;
        };
        self.closing = true;
        self.stop();
        cancel.store(true, Ordering::Release);
        cx.notify();
        false
    }

    fn new(
        owner: WindowHandle<Desk>,
        selected: HashSet<Source>,
        identify: bool,
        recognize: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let search = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let mut s = Self {
            owner,
            inputs: vec![],
            selected,
            identify,
            recognize,
            target: crate::i18n::code().into(),
            search,
            picker: false,
            settings_open: false,
            settings_focus: cx.focus_handle(),
            section: 0,
            sidebar: Default::default(),
            settings_scroll: Default::default(),
            source_menu: None,
            scanning: false,
            status: String::new(),
            control: None,
            capture_start: None,
            latency: None,
            cancel: None,
            stopping: false,
            closing: false,
            names: vec![],
            phrases: Default::default(),
            scroll: Default::default(),
            tx,
            rx,
        };
        s.scan();
        cx.spawn(async move |this, cx| {
            let mut interval = 80;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(interval))
                    .await;
                let Ok(active) = this.update(cx, |s, cx| {
                    while let Ok(e) = s.rx.try_recv() {
                        s.apply(e);
                        cx.notify();
                    }
                    s.cancel.is_some()
                }) else {
                    break;
                };
                interval = if active { 25 } else { 80 };
            }
        })
        .detach();
        s
    }
    fn scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(LiveEvent::Sources(discover()));
        });
    }
    fn start(&mut self, cx: &App) -> Result<()> {
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .filter(|i| self.selected.contains(&i.source))
            .cloned()
            .collect();
        ensure!(!inputs.is_empty(), "Sélectionnez au moins une source");
        let owner = self.owner.read(cx)?;
        let mut settings = owner.settings.clone();
        settings["diarization"] = json!(self.identify);
        settings["recognize"] = json!(self.identify && self.recognize);
        if self.identify {
            ensure!(
                voice_models_ready(&settings),
                "{}",
                tr("Installez les modèles de voix dans les paramètres.")
            );
        }
        let profiles = if self.identify && settings["recognize"] == true {
            owner.db.profiles(true)?
        } else {
            Vec::new()
        };
        for (key, file, msg) in [
            (
                "whisper",
                "model.bin",
                "Choisissez d’abord un modèle de transcription.",
            ),
            (
                "translation",
                "model.bin",
                "Installez le modèle de traduction dans les paramètres.",
            ),
        ] {
            ensure!(
                settings[key]
                    .as_str()
                    .is_some_and(|v| std::path::Path::new(v).join(file).is_file()),
                "{}",
                tr(msg)
            );
        }
        let gains: Vec<_> = inputs
            .iter()
            .map(|i| (i.source.clone(), owner.gain_db(&i.source)))
            .collect();
        let temp = tempfile::Builder::new().prefix("rikai-live-").tempdir()?;
        let dir = temp.path().join("audio");
        let params = json!({"directory":dir,"settings":settings,"tracks":inputs.len(),"target":self.target,"profiles":profiles});
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.control = None;
        self.stopping = false;
        self.picker = false;
        self.settings_open = false;
        self.phrases.clear();
        self.latency = None;
        self.capture_start = None;
        self.names = inputs.iter().map(|i| i.name.clone()).collect();
        self.status = "Chargement du modèle de transcription".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let (ready, start) = mpsc::channel::<Control>();
            let stop = cancel.clone();
            let capture = thread::spawn(move || -> Result<()> {
                if let Ok(ctl) = start.recv() {
                    let result = audio_capture::record_live(&dir, inputs, ctl);
                    if result.is_err() {
                        stop.store(true, Ordering::Release);
                    }
                    result?;
                }
                Ok(())
            });
            let active = std::cell::RefCell::new(None::<Control>);
            let result = crate::worker::run(0, "listen", params, cancel.clone(), |event| {
                if event.live.as_ref().is_some_and(|v| v["ready"] == true)
                    && active.borrow().is_none()
                    && !cancel.load(Ordering::Acquire)
                {
                    let started = Instant::now();
                    let ctl = Control::default();
                    for (source, db) in &gains {
                        ctl.set_gain_db(source, *db);
                    }
                    active.replace(Some(ctl.clone()));
                    let _ = tx.send(LiveEvent::Capture(ctl.clone(), started));
                    let _ = ready.send(ctl);
                }
                let _ = tx.send(LiveEvent::Progress(event));
            });
            if let Some(ctl) = active.into_inner() {
                ctl.stop();
            }
            drop(ready);
            let result = capture
                .join()
                .map_err(|_| anyhow::anyhow!("Interruption du thread audio"))
                .and_then(|r| r)
                .and(result.map(|_| ()));
            drop(temp);
            let _ = tx.send(LiveEvent::Finished(result));
        });
        Ok(())
    }
    fn stop(&mut self) {
        self.stopping = true;
        if let Some(c) = &self.control {
            c.stop();
        } else if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
        self.status = "Finalisation".into();
    }
    fn apply(&mut self, e: LiveEvent) {
        match e {
            LiveEvent::Sources(r) => {
                self.scanning = false;
                match r {
                    Ok(v) => {
                        self.selected
                            .retain(|s| v.iter().any(|input| &input.source == s));
                        self.inputs = v;
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            LiveEvent::Capture(c, started) => {
                self.capture_start = Some(started);
                if self.stopping {
                    c.stop();
                }
                self.control = Some(c);
            }
            LiveEvent::Finished(r) => {
                self.control = None;
                self.status = match r {
                    Ok(()) => "Terminé".into(),
                    Err(_)
                        if self.stopping
                            && self
                                .cancel
                                .as_ref()
                                .is_some_and(|c| c.load(Ordering::Acquire)) =>
                    {
                        "Traitement annulé".into()
                    }
                    Err(e) => {
                        self.settings_open = true;
                        e.to_string()
                    }
                };
                self.cancel = None;
                self.stopping = false;
            }
            LiveEvent::Progress(update) => {
                if !self.stopping {
                    self.status = update.stage;
                }
                let Some(v) = update.live else { return };
                if let Some(edit) = v.get("voice_update") {
                    if let (Some(start), Some(track), Some(part)) = (
                        edit["start"].as_u64(),
                        edit["track"].as_u64(),
                        edit["part"].as_u64(),
                    ) && let Some(row) = self
                        .phrases
                        .iter_mut()
                        .find(|p| p.confirmed && p.key == (start, track as usize, part))
                    {
                        row.speaker = speaker_label(self.names.get(track as usize), edit);
                    }
                    return;
                }
                let (Some(track), Some(start), Some(text)) =
                    (v["track"].as_u64(), v["start"].as_u64(), v["text"].as_str())
                else {
                    return;
                };
                let track = track as usize;
                let key = (start, track, v["part"].as_u64().unwrap_or(0));
                let index = self.phrases.iter().rposition(|p| p.key == key);
                if text.is_empty() {
                    if let Some(i) = index {
                        self.phrases.remove(i);
                    }
                    return;
                }
                let now = Instant::now();
                let i = index.unwrap_or_else(|| {
                    let pos = self.phrases.partition_point(|phrase| phrase.key < key);
                    self.phrases.insert(
                        pos,
                        Phrase {
                            key,
                            confirmed: false,
                            animate_translation: false,
                            speaker: String::new(),
                            original: String::new(),
                            translated: None,
                            appeared: now,
                            previous_prefix: 0,
                        },
                    );
                    pos
                });
                let phrase = &mut self.phrases[i];
                phrase.speaker = speaker_label(self.names.get(track), &v);
                phrase.confirmed = v["final"] == true;
                if phrase.confirmed
                    && let (Some(start), Some(frame)) =
                        (self.capture_start, v["speech_end"].as_u64())
                {
                    self.latency = start.elapsed().checked_sub(Duration::from_secs_f64(
                        frame as f64 / f64::from(audio_capture::RATE),
                    ));
                }
                if text != phrase.original {
                    phrase.previous_prefix = text
                        .split_word_bound_indices()
                        .zip(phrase.original.split_word_bounds())
                        .take_while(|((_, a), b)| a == b)
                        .map(|((i, a), _)| i + a.len())
                        .last()
                        .unwrap_or(0);
                    phrase.translated = None;
                    phrase.original = text.into();
                    phrase.appeared = now;
                }
                if let Some(output) = v["translated"].as_str()
                    && phrase
                        .translated
                        .as_ref()
                        .is_none_or(|(current, _)| current != output)
                {
                    phrase.animate_translation = phrase.confirmed;
                    phrase.translated = Some((output.into(), now));
                }
                if self.phrases.len() > 160 {
                    self.phrases.pop_front();
                }
                self.scroll.scroll_to_bottom();
            }
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if let Some(c) = &self.control {
            c.stop();
        }
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Release);
        }
    }
}

pub(in crate::ui) fn voice_models_ready(settings: &Value) -> bool {
    ["segmentation", "embedding"].iter().all(|key| {
        settings[*key]
            .as_str()
            .is_some_and(|v| std::path::Path::new(v).is_file())
    })
}

// One shaped text run per colour, with grapheme boundaries for combining marks
// and non-Latin scripts. Settled lines allocate no animation frames.
fn phrase_text(phrase: &Phrase, p: Palette, w: &Window, cx: &App) -> AnyElement {
    let muted = rgb_to_hsla(rgb(p.muted));
    let now = Instant::now();
    if phrase.translated.is_none() {
        let mut end = phrase.original.len();
        let fraction = phrase.appeared.elapsed().as_secs_f32() / 0.10;
        if !cx.reduce_motion() && fraction < 1. {
            let words = &phrase.original[phrase.previous_prefix..];
            let count = words.split_word_bounds().count();
            let take = 1 + (glide(fraction) * count as f32) as usize;
            end = phrase.previous_prefix
                + words
                    .split_word_bounds()
                    .take(take)
                    .map(str::len)
                    .sum::<usize>();
            w.request_animation_frame();
        }
        return div()
            .text_color(muted)
            .child(phrase.original[..end].to_owned())
            .into_any_element();
    }
    let (target, start) = phrase.translated.as_ref().unwrap();
    let elapsed = now.saturating_duration_since(*start).as_secs_f32();
    if !phrase.animate_translation || cx.reduce_motion() || elapsed >= 0.12 {
        return div().child(target.clone()).into_any_element();
    }
    w.request_animation_frame();
    let a: Vec<_> = phrase.original.graphemes(true).collect();
    let b: Vec<_> = target.graphemes(true).collect();
    let count = a.len().max(b.len());
    let mut out = String::new();
    let mut colours = Vec::new();
    for i in 0..count {
        let t = glide(((elapsed - 0.08 * i as f32 / count as f32) / 0.04).clamp(0., 1.));
        let original = a.get(i).copied().unwrap_or("");
        let dest = b.get(i).copied().unwrap_or("");
        let g = if t == 0. {
            original
        } else if t < 0.53 && !dest.trim().is_empty() {
            ["·", "∕", "∙"][(i + (elapsed * 24.) as usize) % 3]
        } else {
            dest
        };
        let first = out.len();
        out.push_str(g);
        if !g.is_empty() && t < 1. {
            colours.push((
                first..out.len(),
                HighlightStyle {
                    color: Some(muted.lerp(&rgb_to_hsla(rgb(p.text)), t)),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(out)
        .with_highlights(colours)
        .into_any_element()
}

impl Live {
    fn dismiss_preferences(&mut self) {
        self.settings_open = false;
        self.source_menu = None;
        self.picker = false;
    }
    fn preferences(&self, visibility: f32, w: &Window, cx: &Context<Self>) -> Div {
        const TABS: &[&str] = &["Audio", "Traduction", "Voix"];
        let p = *cx.global::<Palette>();
        let busy = self.cancel.is_some();
        let action = |title: &str, style, icon| control(title, style, icon, false, cx);
        let mut panel = col();
        if self.section == 0 {
            panel = panel.child(row().justify_between().child(label("Sources audio")).when(
                !busy,
                |d| {
                    d.child(
                        action("Actualiser", Button::Quiet, None).on_click(cx.listener(
                            |s, _, _, cx| {
                                s.scan();
                                cx.notify();
                            },
                        )),
                    )
                },
            ));
            for kind in [SourceMenu::Apps, SourceMenu::Mic] {
                panel = panel.child(preferences_panel::source_field(
                    kind,
                    self.source_menu == Some(kind),
                    &self.inputs,
                    &self.selected,
                    self.scanning,
                    busy,
                    |_, _| None,
                    cx,
                    |s, k| {
                        s.source_menu = if s.source_menu == Some(k) {
                            None
                        } else {
                            Some(k)
                        }
                    },
                    |s, k, src| {
                        if s.cancel.is_none() && s.inputs.iter().any(|i| i.source == src) {
                            k.toggle(&mut s.selected, src);
                            if k == SourceMenu::Mic {
                                s.source_menu = None;
                            }
                        }
                    },
                ));
            }
        } else if self.section == 1 {
            panel = panel.child(label("Langue cible"));
            let lang = crate::i18n::language_name(&self.target);
            panel = if busy {
                panel.child(lang.to_owned())
            } else {
                panel.child(
                    action(lang, Button::Secondary, None)
                        .justify_between()
                        .child(glyph(CHEVRON, 14., p.muted))
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.picker = !s.picker;
                            if s.picker {
                                s.search.update(cx, |field, cx| field.emplace("", cx));
                            }
                            cx.notify();
                        })),
                )
            };
            if self.picker && !busy {
                let q = self.search.read(cx).as_str().trim().to_lowercase();
                let mut options = col().gap_0();
                for r in crate::i18n::languages()
                    .iter()
                    .filter(|r| r.iter().any(|v| v.to_lowercase().contains(&q)))
                {
                    let code = r[0].clone();
                    options = options.child(
                        action(crate::i18n::language_name(&code), Button::Quiet, None)
                            .justify_start()
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.target = code.clone();
                                s.picker = false;
                                cx.notify();
                            })),
                    );
                }
                panel = panel
                    .child(input(
                        cx,
                        &self.search,
                        "live-language-search",
                        "Rechercher une langue",
                    ))
                    .child(ScrollArea::new("live-targets", options).limited(170.));
            }
        } else {
            panel = panel.child(
                checkbox(
                    tr("Identifier les intervenants").to_owned(),
                    self.identify,
                    busy,
                    p,
                )
                .id("live-identify")
                .when(!busy, |d| {
                    d.on_click(cx.listener(|s, _, _, cx| {
                        s.identify = !s.identify;
                        cx.notify();
                    }))
                }),
            );
            if self.identify {
                panel = panel.child(
                    checkbox(
                        tr("Proposer les voix connues").to_owned(),
                        self.recognize,
                        busy,
                        p,
                    )
                    .id("live-recognize")
                    .when(!busy, |d| {
                        d.on_click(cx.listener(|s, _, _, cx| {
                            s.recognize = !s.recognize;
                            cx.notify();
                        }))
                    }),
                );
            }
        }
        if !busy && let Ok(owner) = self.owner.read(cx) {
            let missing_whisper = owner.settings["whisper"]
                .as_str()
                .is_none_or(|v| !std::path::Path::new(v).join("model.bin").is_file());
            let missing_translation = owner.settings["translation"]
                .as_str()
                .is_none_or(|v| !std::path::Path::new(v).join("model.bin").is_file());
            let missing_voices = self.identify && !voice_models_ready(&owner.settings);
            if (self.section == 1 && (missing_whisper || missing_translation))
                || (self.section == 2 && missing_voices)
            {
                let voice_tab = self.section == 2;
                panel = panel.child(
                    action("Configurer les modèles", Button::Secondary, None).on_click(
                        cx.listener(move |s, _, _, cx| {
                            let _ = s.owner.update(cx, |desk, w, cx| {
                                w.activate();
                                if !desk.busy() && !desk.release_was_open {
                                    if desk.page != Page::Settings {
                                        let _ = desk.go(Page::Settings);
                                    }
                                    desk.section =
                                        if !voice_tab && missing_whisper { 0 } else { 1 };
                                    if voice_tab || !missing_whisper {
                                        desk.page = Page::ConfirmDownload(if !voice_tab {
                                            "Traduction locale"
                                        } else {
                                            "Identification des voix"
                                        });
                                    }
                                    cx.notify();
                                }
                            });
                        }),
                    ),
                );
            }
        }

        if !self.status.is_empty() {
            panel = panel.child(
                div()
                    .text_color(rgb(p.muted))
                    .child(crate::i18n::message(&self.status)),
            );
        }
        if let Some(l) = self.latency {
            panel = panel.child(format!(
                "{} · {} ms",
                tr("Délai après la parole"),
                l.as_millis()
            ));
        }
        let sidebar = self.sidebar.render(TABS, self.section, w, cx, |s, i| {
            s.sidebar.select(s.section, i, TABS.len());
            s.section = i;
            s.picker = false;
            s.source_menu = None;
            s.settings_scroll.set_offset(Point::default());
        });
        let close = control("Fermer le panneau", Button::Quiet, Some(CLOSE), true, cx)
            .size(px(28.))
            .p_0()
            .flex_shrink_0()
            .on_click(cx.listener(|s, _, _, cx| {
                s.dismiss_preferences();
                cx.notify();
            }));
        let sheet = preferences_panel::sheet(
            tr("Paramètres").into(),
            Some(close),
            Some(sidebar),
            panel,
            self.settings_scroll.clone(),
            p,
        )
        .track_focus(&self.settings_focus)
        .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
            if e.keystroke.key == "escape" {
                if s.picker {
                    s.picker = false;
                } else if s.source_menu.take().is_none() {
                    s.dismiss_preferences();
                }
                cx.stop_propagation();
                cx.notify();
            }
        }));
        preferences_panel::overlay(
            visibility,
            sheet,
            div()
                .id("drawer-backdrop")
                .on_click(cx.listener(|s, _, _, cx| {
                    s.dismiss_preferences();
                    cx.notify();
                })),
        )
    }
}

impl Render for Live {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let busy = self.cancel.is_some();
        if self.closing && !busy {
            w.defer(cx, |w, _| w.remove_window());
        }
        let showing = preferences_panel::visibility(self.settings_open, w, cx);
        let action = |title: &str, style, icon| control(title, style, icon, false, cx);
        let primary = if busy && self.stopping {
            disabled_button("Finalisation", p)
        } else if busy {
            action("Arrêter", Button::Secondary, None).on_click(cx.listener(|s, _, _, cx| {
                s.stop();
                cx.notify();
            }))
        } else {
            action("Enregistrer", Button::Primary, Some(WAVE)).on_click(cx.listener(
                |s, _, w, cx| {
                    if !s.inputs.iter().any(|i| s.selected.contains(&i.source)) {
                        s.section = 0;
                        s.settings_open = true;
                        s.settings_focus.focus(w, cx);
                    } else if let Err(e) = s.start(cx) {
                        s.status = e.to_string();
                        s.section = if s.identify
                            && s.owner
                                .read(cx)
                                .is_ok_and(|owner| !voice_models_ready(&owner.settings))
                        {
                            2
                        } else {
                            1
                        };
                        s.settings_open = true;
                        s.settings_focus.focus(w, cx);
                    }
                    cx.notify();
                },
            ))
        };
        let mut captions = col().p_3().gap_3();
        for phrase in &self.phrases {
            captions = captions.child(
                div()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .child(phrase.speaker.clone())
                            .when(!phrase.confirmed, |d| {
                                d.child(format!(" · {}", tr("Provisoire")))
                            }),
                    )
                    .child(phrase_text(phrase, p, w, cx)),
            );
        }
        let title = div()
            .flex()
            .h_full()
            .min_w_0()
            .items_center()
            .gap(px(6.))
            .window_control_area(WindowControlArea::Drag)
            .child(glyph(TRANSLATE, 15., p.muted))
            .child(label("Traduction en direct").truncate());
        let header = window_frame::header(p, title, !self.settings_open)
            .child(
                control("Paramètres", Button::Secondary, Some(SLIDERS), true, cx).on_click(
                    cx.listener(|s, _, w, cx| {
                        s.settings_open = true;
                        s.settings_focus.focus(w, cx);
                        cx.notify();
                    }),
                ),
            )
            .child(primary)
            .child(window_frame::controls(
                w,
                p,
                true,
                cx.listener(|s, _, w, cx| {
                    if s.can_close(cx) {
                        w.remove_window();
                    }
                }),
            ));
        let mut stream = ScrollArea::new("live-captions", captions);
        stream.handle = Some(self.scroll.clone());
        let content = col()
            .min_h_0()
            .flex_1()
            .px(px(13.))
            .pt(px(8.))
            .pb(px(12.))
            .child(p.panel().flex_1().min_h_0().child(stream));
        let overlay =
            (self.settings_open || showing > 0.001).then(|| self.preferences(showing, w, cx));
        window_frame::surface(p)
            .capture_key_down(cx.listener(|s, ev: &KeyDownEvent, w, cx| {
                if ev.keystroke.key == "tab"
                    && !ev.keystroke.modifiers.control
                    && !ev.keystroke.modifiers.alt
                {
                    let step = if ev.keystroke.modifiers.shift {
                        Window::focus_prev
                    } else {
                        Window::focus_next
                    };
                    step(w, cx);
                    if s.settings_open {
                        let first = w.focused(cx);
                        while !s.settings_focus.contains_focused(w, cx) {
                            step(w, cx);
                            if w.focused(cx) == first {
                                s.settings_focus.focus(w, cx);
                                break;
                            }
                        }
                    }
                    cx.stop_propagation();
                }
            }))
            .child(header)
            .child(content)
            .children(overlay)
    }
}
