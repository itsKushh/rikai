use super::*;
use crate::live_transcript::{FinalTranslations, PhraseKey};
use unicode_segmentation::UnicodeSegmentation;

enum LiveEvent {
    Sources(Result<Vec<Input>>),
    Capture(Control, Instant),
    Progress(crate::worker::Progress),
    Finished(Result<()>),
}

struct Phrase {
    key: PhraseKey,
    confirmed: bool,
    animate_translation: bool,
    speaker: String,
    original: String,
    translated: Option<(String, Instant)>,
    appeared: Instant,
    previous_prefix: usize,
}

pub(in crate::ui) struct Live {
    owner: WeakEntity<Desk>,
    inputs: Vec<Input>,
    selected: HashSet<Source>,
    target: String,
    identify: bool,
    recognize: bool,
    search: Field,
    picker: bool,
    settings_open: bool,

    section: usize,

    source_menu: Option<SourceMenu>,
    scanning: bool,
    status: String,
    control: Option<Control>,
    capture_start: Option<Instant>,
    latency: Option<Duration>,
    cancel: Option<Arc<AtomicBool>>,
    stopping: bool,
    archive_pending: bool,
    project: String,
    names: Vec<String>,
    phrases: std::collections::VecDeque<Phrase>,
    // Caption animation state is bounded; final text stays available for the whole session.
    final_text: FinalTranslations,
    captured_target: String,
    export_message: String,
    export_open: bool,
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
        let _ = w;
        if self.live_view.is_none() {
            let owner = cx.weak_entity();
            self.live_view = Some(cx.new(|cx| {
                Live::new(
                    owner,
                    self.chosen.clone(),
                    self.settings["diarization"] == true,
                    self.settings["recognize"] == true,
                    cx,
                )
            }));
        }
        self.go(Page::Live)
    }
}
impl Live {
    fn show_preferences(&self, cx: &mut Context<Self>) {
        let section = self.section;
        let _ = self.owner.update(cx, |s, cx| {
            s.settings_origin = Page::Live;
            s.page = Page::Settings;
            s.section = 6 + section;
            cx.notify();
        });
    }
    fn new(
        owner: WeakEntity<Desk>,
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

            section: 1,

            source_menu: None,
            scanning: false,
            status: String::new(),
            control: None,
            capture_start: None,
            latency: None,
            cancel: None,
            stopping: false,
            archive_pending: false,
            project: String::new(),
            names: vec![],
            phrases: Default::default(),
            final_text: Default::default(),
            captured_target: String::new(),
            export_message: String::new(),
            export_open: false,
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
                        s.apply(e, cx);
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
    fn start(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let inputs: Vec<_> = self
            .inputs
            .iter()
            .filter(|i| self.selected.contains(&i.source))
            .cloned()
            .collect();
        ensure!(!inputs.is_empty(), "Sélectionnez au moins une source");
        let handle = self.owner.upgrade().context("Fenêtre Rikai introuvable")?;
        let owner = handle.read(cx);
        self.project = owner.project.read(cx).as_str().to_string();
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
        let _ = self.owner.update(cx, |s, cx| {
            s.live_active = true;
            cx.notify();
        });
        self.control = None;
        self.stopping = false;
        self.picker = false;
        self.settings_open = false;
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
    fn apply(&mut self, e: LiveEvent, cx: &mut Context<Self>) {
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
                self.phrases.clear();
                self.final_text = FinalTranslations::default();
                self.archive_pending = true;
                self.captured_target.clone_from(&self.target);
                self.export_message.clear();
                self.export_open = false;
                self.latency = None;
                self.capture_start = Some(started);
                if self.stopping {
                    c.stop();
                }
                self.control = Some(c);
            }
            LiveEvent::Finished(r) => {
                self.phrases.retain(|p| p.confirmed);
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
                let result = self.owner.update(cx, |desk, cx| -> Result<()> {
                    desk.live_active = false;
                    cx.notify();
                    if self.archive_pending && !self.final_text.is_empty() {
                        desk.db.archive_translation(
                            &format!(
                                "{} · {}",
                                tr("Traduction"),
                                crate::i18n::language_name(&self.captured_target)
                            ),
                            &self.project,
                            &self.captured_target,
                            &self.final_text.passages(audio_capture::RATE),
                        )?;
                        desk.refresh("")?;
                    }
                    Ok(())
                });
                self.archive_pending = false;
                if let Err(e) = result.and_then(|r| r) {
                    self.status = e.to_string();
                    self.export_message = e.to_string();
                }
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
                    ) {
                        let key = (start, track as usize, part);
                        let speaker = speaker_label(self.names.get(track as usize), edit);
                        self.final_text.relabel(key, &speaker);
                        if let Some(row) = self
                            .phrases
                            .iter_mut()
                            .find(|p| p.confirmed && p.key == key)
                        {
                            row.speaker = speaker;
                        }
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
                let speaker = speaker_label(self.names.get(track), &v);
                self.final_text.observe(key, text, &speaker, &v);
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
                phrase.speaker = speaker;
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
    fn export_translation(&mut self, fmt: &str, ext: &str, w: &Window, cx: &mut Context<Self>) {
        self.export_open = false;
        let segments = self
            .final_text
            .passages(audio_capture::RATE)
            .into_iter()
            .map(|(start_ms, end_ms, speaker, _, text)| Segment {
                id: 0,
                start_ms,
                end_ms,
                speaker,
                text,
                track: String::new(),
                acoustic_label: String::new(),
                reviewed: false,
                echo_candidate: None,
                words: vec![],
                confidence: None,
            })
            .collect::<Vec<_>>();
        let Ok(text) =
            meeting_storage::export::transcript(tr("Traduction en direct"), &segments, fmt)
        else {
            return;
        };

        let session = self.capture_start;
        let dialog = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .add_filter(ext, &[ext])
            .set_file_name(format!("Rikai-{}.{ext}", self.captured_target));
        cx.spawn(async move |this, cx| {
            let Some(file) = dialog.save_file().await else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    use std::io::Write;
                    let dest = file.path();
                    let mut tmp = tempfile::NamedTempFile::new_in(
                        dest.parent().context("Dossier de destination absent")?,
                    )?;
                    tmp.write_all(text.as_bytes())?;
                    tmp.persist(dest)?;
                    Ok::<_, anyhow::Error>(())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.capture_start == session {
                    s.export_message = match result {
                        Ok(()) => tr("Export enregistré.").to_owned(),
                        Err(e) => format!("{} · {e}", tr("Échec de l’export")),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn export_bar(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let status = if self.export_message.is_empty() {
            crate::i18n::language_name(&self.captured_target)
        } else {
            &self.export_message
        };
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(6.))
            .pt(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(status.to_owned()),
            )
            .child(
                control("Copier la traduction", Button::Quiet, Some(COPY), true, cx).on_click(
                    cx.listener(|s, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            s.final_text.text(audio_capture::RATE),
                        ));
                        s.export_message = tr("Traduction copiée.").to_owned();
                        cx.notify();
                    }),
                ),
            )
            .child(
                control(
                    "Exporter la traduction",
                    Button::Secondary,
                    Some(SAVE),
                    false,
                    cx,
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.export_open = !s.export_open;
                    cx.notify()
                })),
            )
    }

    fn direct_panel(&self, cx: &Context<Self>) -> Div {
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
        if !busy && let Some(handle) = self.owner.upgrade() {
            let owner = handle.read(cx);
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
                            let _ = s.owner.update(cx, |desk, cx| {
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
        panel
    }
}

impl Render for Live {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let busy = self.cancel.is_some();
        if let Some(owner) = self.owner.upgrade() {
            let owner = owner.read(cx);
            if owner.page == Page::Settings && owner.section >= 6 {
                self.section = owner.section - 6;
                return self.direct_panel(cx);
            }
        }
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
                |s, _, _, cx| {
                    if !s.inputs.iter().any(|i| s.selected.contains(&i.source)) {
                        s.section = 0;
                        s.settings_open = true;
                        s.show_preferences(cx);
                    } else if let Err(e) = s.start(cx) {
                        s.status = e.to_string();
                        s.section = if s.identify
                            && s.owner
                                .upgrade()
                                .is_some_and(|owner| !voice_models_ready(&owner.read(cx).settings))
                        {
                            2
                        } else {
                            1
                        };
                        s.settings_open = true;
                        s.show_preferences(cx);
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
        let mut stream = ScrollArea::new("live-captions", captions);
        stream.handle = Some(self.scroll.clone());
        let content = col()
            .min_h_0()
            .flex_1()
            .px(px(13.))
            .pt(px(8.))
            .pb(px(12.))
            .child(p.panel().flex_1().min_h_0().child(stream))
            .when(!busy && !self.final_text.is_empty(), |d| {
                d.child(self.export_bar(cx)).when(self.export_open, |d| {
                    d.child(
                        ScrollArea::new(
                            "live-exports",
                            col().children(meeting_storage::export::FORMATS.map(
                                |(fmt, title, ext)| {
                                    control(
                                        format!("{} · .{ext}", tr(title)),
                                        Button::Quiet,
                                        None,
                                        false,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, w, cx| s.export_translation(fmt, ext, w, cx),
                                    ))
                                },
                            )),
                        )
                        .limited(185.),
                    )
                })
            });
        col()
            .size_full()
            .child(row().justify_end().px_3().py_2().child(primary))
            .child(content)
    }
}
