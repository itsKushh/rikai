use super::*;
use unicode_segmentation::UnicodeSegmentation;

enum LiveEvent {
    Sources(Result<Vec<Input>>),
    Capture(Control),
    Progress(crate::worker::Progress),
    Finished(Result<()>),
}

struct Phrase {
    key: (usize, u64),
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
    search: Field,
    picker: bool,
    scanning: bool,
    status: String,
    control: Option<Control>,
    cancel: Option<Arc<AtomicBool>>,
    stopping: bool,
    closing: bool,
    names: Vec<String>,
    phrases: std::collections::VecDeque<Phrase>,
    scroll: ScrollHandle,
    tx: mpsc::Sender<LiveEvent>,
    rx: mpsc::Receiver<LiveEvent>,
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
        let opt = window_frame::options(
            Live::window_title(),
            size(px(800.), px(580.)),
            size(px(480.), px(340.)),
            cx,
        );
        self.live_window = Some(cx.open_window(opt, |w, cx| {
            let view = cx.new(|cx| Live::new(owner, selected, cx));
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

    fn new(owner: WindowHandle<Desk>, selected: HashSet<Source>, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = mpsc::channel();
        let search = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        let mut s = Self {
            owner,
            inputs: vec![],
            selected,
            target: crate::i18n::code().into(),
            search,
            picker: false,
            scanning: false,
            status: String::new(),
            control: None,
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
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                if this
                    .update(cx, |s, cx| {
                        while let Ok(e) = s.rx.try_recv() {
                            s.apply(e);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
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
        let settings = owner.settings.clone();
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
        let params =
            json!({"directory":dir,"settings":settings,"tracks":inputs.len(),"target":self.target});
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.control = None;
        self.stopping = false;
        self.picker = false;
        self.phrases.clear();
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
                    let ctl = Control::default();
                    for (source, db) in &gains {
                        ctl.set_gain_db(source, *db);
                    }
                    active.replace(Some(ctl.clone()));
                    let _ = tx.send(LiveEvent::Capture(ctl.clone()));
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
                    Ok(v) => self.inputs = v,
                    Err(e) => self.status = e.to_string(),
                }
            }
            LiveEvent::Capture(c) => {
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
                    Err(e) => e.to_string(),
                };
                self.cancel = None;
                self.stopping = false;
            }
            LiveEvent::Progress(update) => {
                if !self.stopping {
                    self.status = update.stage;
                }
                let Some(v) = update.live else { return };
                let (Some(track), Some(start), Some(text)) =
                    (v["track"].as_u64(), v["start"].as_u64(), v["text"].as_str())
                else {
                    return;
                };
                let key = (track as usize, start);
                let index = self.phrases.iter().rposition(|p| p.key == key);
                if text.is_empty() {
                    if let Some(i) = index {
                        self.phrases.remove(i);
                    }
                    return;
                }
                let now = Instant::now();
                let i = index.unwrap_or_else(|| {
                    if self.phrases.len() >= 160 {
                        self.phrases.pop_front();
                    }
                    self.phrases.push_back(Phrase {
                        key,
                        speaker: self.names.get(key.0).cloned().unwrap_or_default(),
                        original: String::new(),
                        translated: None,
                        appeared: now,
                        previous_prefix: 0,
                    });
                    self.phrases.len() - 1
                });
                let phrase = &mut self.phrases[i];
                if text != phrase.original {
                    phrase.previous_prefix = text
                        .split_word_bound_indices()
                        .zip(phrase.original.split_word_bounds())
                        .take_while(|((_, a), b)| a == b)
                        .map(|((i, a), _)| i + a.len())
                        .last()
                        .unwrap_or(0);
                    phrase.original = text.into();
                    phrase.appeared = now;
                }
                if let Some(output) = v["translated"].as_str() {
                    phrase.translated = Some((
                        output.into(),
                        now.max(phrase.appeared + Duration::from_millis(340)),
                    ));
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

// One shaped text run per colour, with grapheme boundaries for combining marks
// and non-Latin scripts. Settled lines allocate no animation frames.
fn phrase_text(phrase: &Phrase, p: Palette, w: &Window, cx: &App) -> AnyElement {
    let muted = rgb_to_hsla(rgb(p.muted));
    let now = Instant::now();
    if phrase.translated.as_ref().is_none_or(|(_, t)| now < *t) {
        if phrase.translated.is_some() {
            w.request_animation_frame();
        }
        let mut end = phrase.original.len();
        let fraction = phrase.appeared.elapsed().as_secs_f32() / 0.27;
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
    if cx.reduce_motion() || elapsed >= 0.78 {
        return div().child(target.clone()).into_any_element();
    }
    w.request_animation_frame();
    let a: Vec<_> = phrase.original.graphemes(true).collect();
    let b: Vec<_> = target.graphemes(true).collect();
    let count = a.len().max(b.len());
    let mut out = String::new();
    let mut colours = Vec::new();
    for i in 0..count {
        let t = glide(((elapsed - 0.57 * i as f32 / count as f32) / 0.21).clamp(0., 1.));
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

impl Render for Live {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let busy = self.cancel.is_some();
        if self.closing && !busy {
            w.defer(cx, |w, _| w.remove_window());
        }
        let action = |title: &str, style, icon| control(title, style, icon, false, cx);
        let mut panel = col().min_h_0().p_4();
        if !busy {
            let mut sources = row();
            for (i, input) in self.inputs.iter().enumerate() {
                let key = input.source.clone();
                sources = sources.child(
                    checkbox(input.name.clone(), self.selected.contains(&key), false, p)
                        .id(("live-source", i))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if !s.selected.remove(&key) {
                                s.selected.insert(key.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            panel = panel
                .child(row().justify_between().child(label("Sources audio")).child(
                    action("Actualiser", Button::Quiet, None).on_click(cx.listener(
                        |s, _, _, cx| {
                            s.scan();
                            cx.notify();
                        },
                    )),
                ))
                .child(sources)
                .child(
                    row().child(label("Langue cible")).child(
                        action(
                            crate::i18n::language_name(&self.target),
                            Button::Secondary,
                            Some(CHEVRON),
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            s.picker = !s.picker;
                            cx.notify();
                        })),
                    ),
                );
            if self.picker {
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
                    .child(
                        div()
                            .id("live-targets")
                            .h(px(110.))
                            .overflow_y_scroll()
                            .child(options),
                    );
            }
        }
        if busy {
            panel = panel
                .child(
                    row()
                        .justify_between()
                        .child(label("Langue cible"))
                        .child(crate::i18n::language_name(&self.target).to_owned()),
                )
                .child(div().text_color(rgb(p.muted)).child(self.names.join(" · ")));
        } else if let Ok(owner) = self.owner.read(cx) {
            let missing_whisper = owner.settings["whisper"]
                .as_str()
                .is_none_or(|v| !std::path::Path::new(v).join("model.bin").is_file());
            let missing_translation = owner.settings["translation"]
                .as_str()
                .is_none_or(|v| !std::path::Path::new(v).join("model.bin").is_file());
            if missing_whisper || missing_translation {
                panel = panel.child(
                    action("Configurer les modèles", Button::Secondary, None).on_click(
                        cx.listener(move |s, _, _, cx| {
                            let _ = s.owner.update(cx, |desk, w, cx| {
                                w.activate();
                                if !desk.busy() && !desk.release_was_open {
                                    if desk.page != Page::Settings {
                                        let _ = desk.go(Page::Settings);
                                    }
                                    desk.section = if missing_whisper { 0 } else { 1 };
                                    if !missing_whisper {
                                        desk.page = Page::ConfirmDownload("Traduction locale");
                                    }
                                    cx.notify();
                                }
                            });
                        }),
                    ),
                );
            }
        }
        let primary = if busy && self.stopping {
            disabled_button("Finalisation", p)
        } else if busy {
            action("Arrêter", Button::Secondary, None).on_click(cx.listener(|s, _, _, cx| {
                s.stop();
                cx.notify();
            }))
        } else if self
            .inputs
            .iter()
            .any(|i| self.selected.contains(&i.source))
        {
            action("Enregistrer", Button::Primary, Some(WAVE)).on_click(cx.listener(
                |s, _, _, cx| {
                    if let Err(e) = s.start(cx) {
                        s.status = e.to_string();
                    }
                    cx.notify();
                },
            ))
        } else {
            disabled_button("Enregistrer", p)
        };
        let mut captions = div()
            .flex()
            .flex_col()
            .id("live-captions")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .gap_3();
        for phrase in &self.phrases {
            captions = captions.child(
                div()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .child(phrase.speaker.clone()),
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
        let header =
            window_frame::header(p, title, true)
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
        let content = col()
            .min_h_0()
            .flex_1()
            .gap(px(16.))
            .px(px(13.))
            .pt(px(8.))
            .pb(px(12.))
            .child(
                p.panel().flex_shrink_0().max_h(relative(0.62)).child(
                    div()
                        .id("live-configuration")
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(panel),
                ),
            )
            .child(p.panel().flex_1().min_h_0().child(captions))
            .when(!self.status.is_empty(), |d| {
                d.child(
                    div()
                        .text_color(rgb(p.muted))
                        .child(crate::i18n::message(&self.status)),
                )
            });
        window_frame::surface(p)
            .capture_key_down(|ev: &KeyDownEvent, w, cx| {
                if ev.keystroke.key == "tab"
                    && !ev.keystroke.modifiers.control
                    && !ev.keystroke.modifiers.alt
                {
                    if ev.keystroke.modifiers.shift {
                        w.focus_prev(cx);
                    } else {
                        w.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            })
            .child(header)
            .child(content)
    }
}
