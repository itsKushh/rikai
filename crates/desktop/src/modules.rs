use super::*;
const MEDIA_TITLE: &str = "Export audio et vidéo vers texte";
const MEDIA_TYPES: &[&str] = &[
    "wav", "mp3", "flac", "m4a", "aac", "aiff", "aif", "ogg", "oga", "opus", "wma", "mp4", "m4v",
    "mkv", "mov", "webm", "avi", "mpeg", "mpg", "mts", "m2ts", "ts", "wmv", "3gp", "mka", "caf",
    "amr", "ac3", "aifc", "vob", "ogv",
];
impl Desk {
    pub(super) fn first_home(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_2xl().child(tr("Enregistrez un moment")))
            .child(
                div()
                    .text_color(rgb(p.muted))
                    .child(tr("Un entretien, un appel ou une discussion à retrouver.")),
            )
            .child(
                row()
                    .gap_2()
                    .my_2()
                    .child(button(
                        "Enregistrer",
                        Button::Primary,
                        Some(WAVE),
                        false,
                        cx,
                        |s, _| s.go(Page::New),
                    ))
                    .child(button(
                        "Modules",
                        Button::Secondary,
                        Some(APPLICATIONS),
                        false,
                        cx,
                        |s, _| s.go(Page::Modules),
                    )),
            )
            .child(window_button(
                "Ou déposez un fichier audio ou vidéo ici",
                Button::Quiet,
                None,
                false,
                cx,
                |s, w, cx| s.pick_media(false, w, cx),
            ))
    }
    pub(super) fn module_cards(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut cards = div().flex().flex_wrap().gap_3();
        for (page, title, desc, icon) in [
            (
                Page::Media,
                MEDIA_TITLE,
                "Transcrire un média et exporter le texte ou les sous-titres.",
                SAVE,
            ),
            (
                Page::Live,
                "Traduction en direct",
                "Traduire les paroles au fil de l’échange.",
                TRANSLATE,
            ),
            (
                Page::Follow,
                "Suivi entre échanges",
                "Comparer les passages de deux enregistrements.",
                SESSION,
            ),
        ] {
            cards = cards.child(
                p.panel()
                    .id(title)
                    .role(Role::Button)
                    .focusable()
                    .tab_stop(true)
                    .aria_label(tr(title))
                    .cursor_pointer()
                    .flex_1()
                    .min_w(px(175.))
                    .p(px(8.))
                    .gap_2()
                    .hover(|d| d.border_color(rgb(p.muted)))
                    .transitions(|t| t.border_color(motion(150)))
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .items_center()
                            .rounded(px(8.))
                            .h(px(104.))
                            .bg(rgb(p.base))
                            .child(if page == Page::Media {
                                row()
                                    .gap_3()
                                    .items_center()
                                    .child(glyph(AUDIO, 21., p.muted))
                                    .child("→")
                                    .child(
                                        col()
                                            .items_center()
                                            .child(glyph(SESSION, 20., p.text))
                                            .child("TXT"),
                                    )
                            } else if page == Page::Live {
                                col()
                                    .gap_1()
                                    .child(
                                        row()
                                            .gap_1()
                                            .child(glyph(TRANSLATE, 14., p.muted))
                                            .child("À demain."),
                                    )
                                    .child(row().gap_1().child("→").child("See you tomorrow."))
                            } else {
                                row()
                                    .gap_3()
                                    .child(glyph(icon, 24., p.muted))
                                    .child("↔")
                                    .child(glyph(icon, 24., p.text))
                            }),
                    )
                    .child(
                        col()
                            .px_2()
                            .pb_2()
                            .gap_1()
                            .child(label(title))
                            .child(div().text_color(rgb(p.muted)).child(tr(desc))),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        let res = if page == Page::Live {
                            s.open_live(w, cx)
                        } else {
                            s.import_config = true;
                            s.go(page)
                        };
                        if let Err(e) = res {
                            s.message = e.to_string()
                        }
                        cx.notify();
                    })),
            );
        }
        cards
    }
    fn pick_media(&mut self, config: bool, w: &Window, cx: &mut Context<Self>) -> Result<()> {
        let dlg = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .add_filter(tr("Audio et vidéo"), MEDIA_TYPES);
        cx.spawn(async move |this, cx| {
            if let Some(f) = dlg.pick_file().await {
                let _ = this.update(cx, |s, cx| s.choose_media(f.path().into(), config, cx));
            }
        })
        .detach();
        Ok(())
    }
    pub(super) fn choose_media(&mut self, path: PathBuf, config: bool, cx: &mut Context<Self>) {
        if self.busy()
            || self.live_active
            || self.project_form.is_some()
            || self.drawer_open()
            || self.release_was_open
        {
            return;
        }
        if !path.is_file()
            || !path
                .extension()
                .and_then(|v| v.to_str())
                .is_some_and(|s| MEDIA_TYPES.contains(&s.to_ascii_lowercase().as_str()))
        {
            self.message = "Choisissez un fichier audio ou vidéo.".into();
            return;
        }
        self.stop_segment();
        self.page = Page::Media;
        self.import_config = config;
        self.picked_media = Some(path.clone());
        self.media_probe = Value::Null;
        self.media_preview = None;
        self.message.clear();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = (|| {
                let dir = tempfile::tempdir()?;
                let info = crate::worker::run(
                    0,
                    "probe_media",
                    json!({"path":path,"thumbnail":dir.path().join("preview.png")}),
                    Arc::new(AtomicBool::new(false)),
                    |_| {},
                )?;
                Ok((dir, info))
            })();
            let _ = tx.send(Event::MediaProbe(path, result));
        });
        cx.notify();
    }
    pub(super) fn media_config(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let pick = window_button(
            if self.picked_media.is_some() {
                "Changer de fichier"
            } else {
                "Choisir un fichier"
            },
            Button::Secondary,
            Some(DESK),
            false,
            cx,
            |s, w, cx| s.pick_media(true, w, cx),
        );
        let Some(path) = &self.picked_media else {
            return col()
                .items_center()
                .py_8()
                .gap_3()
                .child(glyph(AUDIO, 30., p.muted))
                .child(label("Déposez un fichier audio ou vidéo"))
                .child(pick);
        };
        let info = &self.media_probe;
        let thumb = self
            .media_preview
            .as_ref()
            .map(|d| d.path().join("preview.png"))
            .filter(|p| p.is_file());
        let preview = div()
            .w(px(180.))
            .h(px(115.))
            .rounded(px(9.))
            .overflow_hidden()
            .flex()
            .justify_center()
            .items_center()
            .bg(rgb(p.base))
            .child(if let Some(thumb) = thumb {
                img(thumb)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element()
            } else {
                glyph(AUDIO, 32., p.muted).into_any_element()
            });
        let desc = if info.is_null() {
            tr("Lecture des informations…").to_owned()
        } else if let Some(err) = info["error"].as_str() {
            err.into()
        } else {
            format!(
                "{} · {:.1} Mio · {}{}",
                path.extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase(),
                info["size"].as_u64().unwrap_or(0) as f64 / 1048576.,
                time(info["duration_ms"].as_u64().unwrap_or(0)),
                if info["video"] == true {
                    format!(" · {} × {}", info["width"], info["height"])
                } else {
                    String::new()
                }
            )
        };
        let mut body = col().gap_3().child(
            row().gap_4().child(preview).child(
                col()
                    .gap_2()
                    .child(raw_label(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    ))
                    .child(desc)
                    .child(pick),
            ),
        );
        if info["size"].is_number() {
            if self.import_config {
                let active = Page::MODEL_PRESETS
                    .iter()
                    .copied()
                    .find(|name| self.settings["whisper_repo"] == Self::repo_for(name))
                    .map(tr)
                    .unwrap_or(tr("Sélectionner"));
                body = body
                    .child(label("Modèle de transcription"))
                    .child(button(
                        active.to_owned(),
                        Button::Secondary,
                        Some(CHEVRON),
                        false,
                        cx,
                        |s, _| {
                            s.model_choice = !s.model_choice;
                            Ok(())
                        },
                    ))
                    .when(self.model_choice, |d| {
                        d.children(Page::MODEL_PRESETS.map(|preset| {
                            button(preset, Button::Quiet, None, false, cx, move |s, _| {
                                s.model_choice = false;
                                s.settings_origin = Page::Media;
                                s.select_model(preset)
                            })
                        }))
                    });
                body = body
                    .child(self.project_picker(cx))
                    .child(self.preference(Preference::Language, cx))
                    .child(toggle(
                        "Identifier les intervenants",
                        "diarization",
                        self,
                        cx,
                    ))
                    .child(action("Transcrire le fichier", true, cx, |s, cx| {
                        s.start_import(cx)
                    }));
            } else {
                body = body
                    .child(label("Que faire avec ce fichier ?"))
                    .child(button(
                        MEDIA_TITLE,
                        Button::Primary,
                        Some(SAVE),
                        false,
                        cx,
                        |s, _| {
                            s.import_config = true;
                            Ok(())
                        },
                    ));
            }
        }
        body
    }
    fn start_import(&mut self, cx: &App) -> Result<()> {
        ensure!(!self.busy(), "Un travail est déjà actif");
        let path = self
            .picked_media
            .clone()
            .context("Choisissez un fichier audio ou vidéo.")?;
        let name = path
            .file_name()
            .context("Nom de fichier absent")?
            .to_string_lossy()
            .to_string();
        let project = self.project.read(cx).as_str().to_owned();
        let options = self.settings.clone();
        let stage = tempfile::Builder::new()
            .prefix("import-")
            .tempdir_in(self.root.join("audio"))?;
        let cancel = self.begin_work();
        self.stage = "Import audio".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let res = crate::worker::run(
                0,
                "import_media",
                json!({"path":path,"directory":stage.path()}),
                cancel,
                |v| {
                    let _ = tx.send(Event::Stage(v));
                },
            )
            .map(|v| (stage, v));
            let _ = tx.send(Event::Imported(name, project, options, res));
        });
        Ok(())
    }
    pub(super) fn shortcuts_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col().gap_3();
        for (key, title) in [
            ("record_key", "Démarrer un enregistrement"),
            ("clip_key", "Garder les dernières secondes"),
        ] {
            let value = if self.binding == Some(key) {
                "...".into()
            } else {
                self.key_for(key).to_uppercase()
            };
            body = body.child(
                row().justify_between().child(label(title)).child(
                    button(value, Button::Secondary, None, false, cx, move |s, _| {
                        s.binding = if s.binding == Some(key) {
                            None
                        } else {
                            Some(key)
                        };
                        Ok(())
                    })
                    .id(key)
                    .min_w(px(58.)),
                ),
            );
        }
        body.child(
            div()
                .text_color(rgb(cx.global::<Palette>().muted))
                .child(tr("Fenêtre active · Échap annule la saisie.")),
        )
        .child(label("Durée des extraits"))
        .child(self.budget(budgets::Budget::ClipSeconds, cx))
    }
    fn key_for(&self, key: &str) -> &str {
        self.settings[key]
            .as_str()
            .unwrap_or(if key == "record_key" { "f8" } else { "f9" })
    }
    pub(super) fn handle_shortcut(
        &mut self,
        ev: &KeyDownEvent,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let k = &ev.keystroke;
        let code = k.unparse();
        if self.page != Page::Settings || self.section != 4 {
            self.binding = None;
        }
        if let Some(key) = self.binding {
            if ev.is_held {
                return true;
            }
            if k.key == "escape" {
                self.binding = None;
                self.message.clear();
                return true;
            }
            if [
                "tab",
                "enter",
                "backspace",
                "delete",
                "up",
                "down",
                "left",
                "right",
                "f5",
                "f11",
                "f12",
            ]
            .contains(&k.key.as_str())
                || ["alt-f4", "ctrl-w", "ctrl-r"].contains(&code.as_str())
            {
                self.message = "Cette touche est réservée.".into();
                return true;
            }
            if self.key_for(if key == "clip_key" {
                "record_key"
            } else {
                "clip_key"
            }) == code
            {
                self.message = "Ce raccourci est déjà utilisé.".into();
                return true;
            }
            self.settings[key] = json!(code);
            self.binding = None;
            self.message = self
                .save_settings()
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            return true;
        }
        if ev.is_held
            || self.page == Page::Live
            || self.drawer_open()
            || self.release_was_open
            || self.project_form.is_some()
            || matches!(self.page, Page::ConfirmDownload(_) | Page::StorageChange)
        {
            return false;
        }
        if [
            &self.title,
            &self.project,
            &self.search,
            &self.record_context,
            &self.edit,
            &self.browse_query,
            &self.folder_name,
            &self.profile_name,
            &self.description,
            &self.tags,
            &self.filter,
            &self.day,
            &self.language_search,
        ]
        .iter()
        .any(|f| f.read(cx).focus_handle(cx).is_focused(w))
        {
            return false;
        }
        let result = if code == self.key_for("clip_key") && self.ctl.is_some() {
            self.mark_excerpt()
        } else if code == self.key_for("record_key") && !self.busy() {
            if self.chosen.is_empty() {
                self.go(Page::New).map(|_| self.quick_start = true)
            } else {
                self.page = Page::New;
                self.start(cx)
            }
        } else {
            return false;
        };
        if let Err(e) = result {
            self.message = e.to_string()
        }
        true
    }
    pub(super) fn mark_excerpt(&mut self) -> Result<()> {
        let end = self
            .ctl
            .as_ref()
            .context("Aucun enregistrement en cours")?
            .elapsed_ms() as i64;
        ensure!(
            end > 0 && !self.stopping,
            "Enregistrement en cours de préparation"
        );
        let ms = self.settings["clip_seconds"]
            .as_i64()
            .unwrap_or(30)
            .clamp(1, 90)
            * 1000;
        self.db.mark_clip(self.id, (end - ms).max(0), end)?;
        self.message = "Extrait ajouté au carnet.".into();
        Ok(())
    }
    pub(super) fn bookmarks_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col().gap_2();
        let clips = self.db.marked_clips(self.id).unwrap_or_default();
        if clips.is_empty() {
            return body.child(label("Aucun extrait marqué."));
        }
        for (a, b) in clips {
            body = body.child(
                row()
                    .gap_2()
                    .child(format!("{} – {}", time(a as u64), time(b as u64)))
                    .child(
                        button(
                            "Écouter l’extrait",
                            Button::Quiet,
                            Some(PLAY),
                            true,
                            cx,
                            move |s, _| s.play_mark(a, b),
                        )
                        .id(("play-mark", b as usize)),
                    )
                    .child(
                        window_button(
                            "Exporter en WAV",
                            Button::Secondary,
                            Some(SAVE),
                            false,
                            cx,
                            move |s, w, cx| {
                                let dir = s.audio_dir(s.id);
                                let dialog = rfd::AsyncFileDialog::new()
                                    .set_parent(w)
                                    .set_file_name(format!("Rikai-{}-{a}.wav", s.id))
                                    .add_filter("WAV", &["wav"]);
                                cx.spawn(async move |desk, cx| {
                                    if let Some(f) = dialog.save_file().await {
                                        let result = cx
                                            .background_executor()
                                            .spawn(async move {
                                                audio_capture::export_mark(&dir, f.path(), a, b)
                                            })
                                            .await;
                                        let _ = desk.update(cx, |s, cx| {
                                            s.message =
                                                result.err().map(|e| e.to_string()).unwrap_or_else(
                                                    || "Extrait WAV enregistré.".into(),
                                                );
                                            cx.notify()
                                        });
                                    }
                                })
                                .detach();
                                Ok(())
                            },
                        )
                        .id(("save-mark", b as usize)),
                    ),
            );
        }
        body
    }
    pub(super) fn subtitle_rows(&self, cx: &Context<Self>) -> Div {
        if self.segments.is_empty() {
            col().child(tr("Aucune intervention transcrite."))
        } else {
            self.dialogue_list(cx)
        }
    }
}
impl Desk {
    pub(super) fn folder_title(&self) -> String {
        self.folders
            .iter()
            .find(|(id, _)| Some(*id) == self.project_folder)
            .map(|(_, n)| n.clone())
            .unwrap_or_default()
    }
    pub(super) fn project_picker(&self, cx: &Context<Self>) -> Div {
        let follow = self.page == Page::Follow;
        let selected = if follow {
            self.folder_title()
        } else {
            self.project.read(cx).as_str().to_string()
        };
        let empty = if follow {
            "Tous les projets"
        } else {
            "Sans projet"
        };
        let mut field = col().gap_1().child(label("Projet")).child(button(
            if selected.is_empty() {
                tr(empty).into()
            } else {
                selected
            },
            Button::Secondary,
            Some(CHEVRON),
            false,
            cx,
            |s, _| {
                s.project_choice = !s.project_choice;
                Ok(())
            },
        ));
        if self.project_choice {
            let mut options = col().gap_1();
            for (id, name) in
                std::iter::once((0, tr(empty).to_owned())).chain(self.folders.iter().cloned())
            {
                let val = if id == 0 { String::new() } else { name.clone() };
                options = options.child(
                    button(name, Button::Quiet, None, false, cx, move |s, cx| {
                        if follow {
                            s.set_folder((id != 0).then_some(id))?;
                            s.selected_items.clear();
                        } else {
                            s.project.update(cx, |f, cx| f.emplace(&val, cx));
                        }
                        s.project_choice = false;
                        Ok(())
                    })
                    .id(("pick-project", id as usize))
                    .justify_start(),
                );
            }
            field = field.child(ScrollArea::new("project-menu", options).limited(160.));
        }
        field
    }
    fn set_folder(&mut self, id: Option<i64>) -> Result<()> {
        self.folder_items = match id {
            Some(id) => self.db.project_items(id)?.into_iter().collect(),
            None => HashSet::new(),
        };
        self.project_folder = id;
        Ok(())
    }
    pub(super) fn library_header_action(&self, cx: &Context<Self>) -> Stateful<Div> {
        let create = self.page != Page::Project;
        button(
            if create {
                "Créer un projet"
            } else {
                "Ajouter des éléments"
            },
            Button::Secondary,
            Some(FOLDER),
            false,
            cx,
            move |s, cx| {
                s.selected_items.clear();
                s.folder_name.update(cx, |f, cx| f.emplace("", cx));
                s.project_form = Some(create);
                s.message.clear();
                Ok(())
            },
        )
    }
    pub(super) fn project_modal(&self, cx: &Context<Self>) -> Div {
        let create = self.project_form == Some(true);
        let p = *cx.global::<Palette>();
        let mut fields = col().gap_2();
        if create {
            fields = fields.child(label("Nom du projet")).child(input(
                cx,
                &self.folder_name,
                "folder-name",
                "Nom du projet",
            ));
        }
        fields = fields.child(label("Enregistrements et traductions"));
        let mut items = col();
        for m in self
            .library
            .iter()
            .filter(|m| create || !self.folder_items.contains(&m.id))
        {
            let id = m.id;
            items = items.child(
                checkbox(m.title.clone(), self.selected_items.contains(&id), false, p)
                    .id(("add-item", id as usize))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if !s.selected_items.remove(&id) {
                            s.selected_items.insert(id);
                        }
                        cx.notify();
                    })),
            );
        }
        fields = fields.child(ScrollArea::new("choose-project-items", items).limited(230.));
        if !self.message.is_empty() {
            fields = fields.child(crate::i18n::message(&self.message))
        }
        let sheet = p
            .panel()
            .id("project-dialog")
            .role(Role::Dialog)
            .aria_modal(true)
            .track_focus(&self.drawer_focus)
            .occlude()
            .p_4()
            .gap_3()
            .w(px(440.))
            .max_w(relative(0.94))
            .child(label(if create {
                "Créer un projet"
            } else {
                "Ajouter des éléments"
            }))
            .child(fields)
            .child(
                row()
                    .justify_end()
                    .gap_2()
                    .child(action("Annuler", false, cx, |s, _| {
                        s.project_form = None;
                        Ok(())
                    }))
                    .child(
                        if (create && self.folder_name.read(cx).as_str().trim().is_empty())
                            || (!create && self.selected_items.is_empty())
                        {
                            disabled_button("Enregistrer", p)
                        } else {
                            action("Enregistrer", true, cx, move |s, cx| {
                                let id = s.db.save_project(
                                    if create { None } else { s.project_folder },
                                    s.folder_name.read(cx).as_str(),
                                    &s.selected_items.iter().copied().collect::<Vec<_>>(),
                                )?;
                                s.project_form = None;
                                s.selected_items.clear();
                                s.refresh("")?;
                                s.set_folder(Some(id))?;
                                s.page = Page::Project;
                                Ok(())
                            })
                        },
                    ),
            );
        preferences_panel::overlay(
            1.,
            sheet,
            div()
                .id("project-backdrop")
                .on_click(cx.listener(|s, _, _, cx| {
                    s.project_form = None;
                    cx.notify()
                })),
        )
    }
    fn archive_search_field(&self, id: &'static str, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        row()
            .items_center()
            .gap_2()
            .child(glyph(SESSION, 16., p.muted))
            .child(div().flex_1().child(input(
                cx,
                &self.search,
                id,
                "Titre, paroles ou intervenant",
            )))
            .when(!self.archive_search.is_empty(), |d| {
                d.child(button(
                    "Effacer la recherche",
                    Button::Quiet,
                    Some(CLOSE),
                    true,
                    cx,
                    |s, cx| {
                        s.search.update(cx, |f, cx| f.emplace("", cx));
                        Ok(())
                    },
                ))
            })
    }
    pub(super) fn archive_page(&self, cx: &Context<Self>) -> Div {
        let mut body = col()
            .gap_2()
            .child(self.archive_search_field("library-search", cx));
        let q = self.archive_search.to_lowercase();
        for (id, name) in &self.folders {
            if !name.to_lowercase().contains(&q) {
                continue;
            }
            let id = *id;
            body = body.child(
                button(
                    name.clone(),
                    Button::Quiet,
                    Some(FOLDER),
                    false,
                    cx,
                    move |s, cx| {
                        s.set_folder(Some(id))?;
                        s.search.update(cx, |f, cx| f.emplace("", cx));
                        s.page = Page::Project;
                        Ok(())
                    },
                )
                .id(("project", id as usize))
                .justify_start(),
            );
        }
        body.child(self.meetings_view(cx, usize::MAX, true))
            .child(button(
                "Filtres de recherche",
                Button::Quiet,
                Some(SLIDERS),
                false,
                cx,
                |s, _| {
                    s.search_more = !s.search_more;
                    Ok(())
                },
            ))
            .when(self.search_more, |d| {
                d.child(input(cx, &self.filter, "filter", "Projet ou tag"))
                    .child(input(cx, &self.day, "day", "Date · AAAA-MM ou AAAA-MM-JJ"))
                    .child(action("Rechercher", false, cx, |s, cx| s.search_db(cx)))
                    .children(
                        self.hits
                            .iter()
                            .map(|h| self.source_button(h.meeting, h.segment, h.text.clone(), cx)),
                    )
            })
    }
    pub(super) fn project_page(&self, cx: &Context<Self>) -> Div {
        col()
            .gap_3()
            .child(self.archive_search_field("folder-search", cx))
            .child(self.meetings_view(cx, usize::MAX, true))
            .child(
                row()
                    .gap_2()
                    .child(button(
                        "Enregistrer dans ce projet",
                        Button::Secondary,
                        Some(WAVE),
                        false,
                        cx,
                        |s, cx| {
                            let name = s.folder_title();
                            s.project.update(cx, |f, cx| f.emplace(&name, cx));
                            s.go(Page::New)
                        },
                    ))
                    .child(button(
                        "Suivi entre échanges",
                        Button::Quiet,
                        Some(SESSION),
                        false,
                        cx,
                        |s, _| {
                            s.selected_items.clear();
                            s.compared.clear();
                            s.go(Page::Follow)
                        },
                    )),
            )
    }
    fn source_button(
        &self,
        id: i64,
        segment: i64,
        text: String,
        cx: &Context<Self>,
    ) -> impl IntoElement + Styled + use<> {
        button(text, Button::Quiet, None, false, cx, move |s, _| {
            s.detail(id)?;
            s.tab = 1;
            if let Some(pos) = s.segments.iter().position(|seg| seg.id == segment) {
                s.dialogues.scroll_to_reveal_item(pos)
            }
            Ok(())
        })
        .id(("source-segment", segment as usize))
        .justify_start()
    }
    pub(super) fn follow_page(&self, cx: &Context<Self>) -> Div {
        if !self.compared.is_empty() {
            let mut columns = row().items_start().gap_4();
            for (title, id, segments) in &self.compared {
                let mut block = col()
                    .min_w_0()
                    .flex_1()
                    .gap_2()
                    .child(raw_label(title.clone()));
                for s in segments.iter().skip(self.compare_offset).take(30) {
                    block = block.child(
                        self.source_button(
                            *id,
                            s.id,
                            format!("{} · {}\n{}", time(s.start_ms as u64), s.speaker, s.text),
                            cx,
                        )
                        .h_auto()
                        .whitespace_normal(),
                    );
                }
                columns = columns.child(block);
            }
            let mut paired = col().gap_2();
            for (term, a, b) in &self.shared_phrases {
                let left = &self.compared[0];
                let right = &self.compared[1];
                let l = &left.2[*a];
                let r = &right.2[*b];
                paired = paired.child(raw_label(term.clone())).child(
                    row()
                        .gap_2()
                        .child(
                            self.source_button(
                                left.1,
                                l.id,
                                format!("{} · {}", left.0, time(l.start_ms as u64)),
                                cx,
                            )
                            .flex_1(),
                        )
                        .child(
                            self.source_button(
                                right.1,
                                r.id,
                                format!("{} · {}", right.0, time(r.start_ms as u64)),
                                cx,
                            )
                            .flex_1(),
                        ),
                );
            }
            return col()
                .gap_3()
                .child(action("Modifier la sélection", false, cx, |s, _| {
                    s.compared.clear();
                    Ok(())
                }))
                .when(!self.shared_phrases.is_empty(), |d| {
                    d.child(label("Expressions communes")).child(paired)
                })
                .child(columns)
                .child(
                    row()
                        .gap_2()
                        .when(self.compare_offset > 0, |d| {
                            d.child(action("Passages précédents", false, cx, |s, _| {
                                s.compare_offset = s.compare_offset.saturating_sub(30);
                                Ok(())
                            }))
                        })
                        .when(
                            self.compared
                                .iter()
                                .any(|(_, _, v)| v.len() > self.compare_offset + 30),
                            |d| {
                                d.child(action("Passages suivants", false, cx, |s, _| {
                                    s.compare_offset += 30;
                                    Ok(())
                                }))
                            },
                        ),
                );
        }
        let p = *cx.global::<Palette>();
        let mut body = col()
            .gap_2()
            .child(self.project_picker(cx))
            .child(self.archive_search_field("follow-search", cx));
        for m in &self.library {
            if !self.transcribed.contains(&m.id)
                || self.project_folder.is_some() && !self.folder_items.contains(&m.id)
                || !self.search_matches(m.id)
            {
                continue;
            }
            let id = m.id;
            let on = self.selected_items.contains(&id);
            let disabled = !on && self.selected_items.len() >= 2;
            body = body.child(
                checkbox(m.title.clone(), on, disabled, p)
                    .id(("compare-record", id as usize))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if !s.selected_items.remove(&id) && s.selected_items.len() < 2 {
                            s.selected_items.insert(id);
                        }
                        cx.notify();
                    })),
            );
        }
        body.child(format!("{} / 2", self.selected_items.len()))
            .child(if self.selected_items.len() == 2 {
                action("Comparer les échanges", true, cx, |s, _| {
                    s.compare_offset = 0;
                    let mut ids = s.selected_items.iter().copied().collect::<Vec<_>>();
                    ids.sort();
                    s.compared = ids
                        .iter()
                        .map(|id| Ok((s.db.metadata(*id)?.0, *id, s.db.segments(*id)?)))
                        .collect::<Result<_>>()?;
                    s.shared_phrases =
                        meeting_storage::shared_phrases(&s.compared[0].2, &s.compared[1].2);
                    Ok(())
                })
            } else {
                disabled_button("Comparer les échanges", p)
            })
    }
    pub(super) fn search_matches(&self, id: i64) -> bool {
        self.archive_search.is_empty() || self.matches.contains(&id)
    }
}
