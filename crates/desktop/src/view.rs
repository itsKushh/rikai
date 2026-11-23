use super::appearance::*;
use super::*;
use gpui::Div;

#[derive(Clone, Copy)]
enum Button {
    Primary,
    Secondary,
    Quiet,
    Selected,
}

fn action(
    label: impl Into<String>,
    primary: bool,
    cx: &Context<Desk>,
    f: impl Fn(&mut Desk, &mut Context<Desk>) -> Result<()> + 'static,
) -> Stateful<Div> {
    button(
        label,
        if primary {
            Button::Primary
        } else {
            Button::Secondary
        },
        None,
        false,
        cx,
        f,
    )
}

fn button(
    label: impl Into<String>,
    style: Button,
    icon: Option<&'static str>,
    icon_only: bool,
    cx: &Context<Desk>,
    f: impl Fn(&mut Desk, &mut Context<Desk>) -> Result<()> + 'static,
) -> Stateful<Div> {
    window_button(label, style, icon, icon_only, cx, move |s, _, cx| f(s, cx))
}

fn window_button(
    label: impl Into<String>,
    style: Button,
    icon: Option<&'static str>,
    icon_only: bool,
    cx: &Context<Desk>,
    f: impl Fn(&mut Desk, &mut Window, &mut Context<Desk>) -> Result<()> + 'static,
) -> Stateful<Div> {
    let label = label.into();
    let p = *cx.global::<Palette>();
    let (bg, fg) = match style {
        Button::Primary => (p.primary, p.on_primary),
        Button::Secondary => (p.control, p.text),
        Button::Selected => (p.selected, p.text),
        Button::Quiet => (p.base, p.muted),
    };
    div()
        .id(SharedString::from(label.clone()))
        .aria_label(label.clone())
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .min_h(px(28.))
        .px(px(9.))
        .py(px(3.))
        .rounded(px(9.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .role(gpui::Role::Button)
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .when(!matches!(style, Button::Quiet), |d| d.bg(rgb(bg)))
        .border_1()
        .border_color(if matches!(style, Button::Quiet) {
            transparent_black()
        } else {
            rgb_to_hsla(rgb(p.edge))
        })
        .text_color(rgb(fg))
        .hover(move |s| {
            s.border_color(rgb(p.muted))
                .bg(rgb(if matches!(style, Button::Quiet) {
                    p.control
                } else {
                    bg
                }))
        })
        .focus_visible(move |s| s.border_color(rgb(p.text)))
        .active(|s| s.opacity(0.8))
        .when_some(icon, |s, icon| s.child(glyph(icon, 15., fg)))
        .when(!icon_only, |s| s.child(label))
        .on_click(cx.listener(move |s, _, w, cx| {
            s.message.clear();
            if let Err(e) = f(s, w, cx) {
                s.message = format!("{e:#}");
            }
            cx.notify();
        }))
}
fn col() -> Div {
    div().min_w_0().flex().flex_col().gap(px(9.))
}
fn row() -> Div {
    div().flex().flex_wrap().items_center().gap_2()
}
fn table_row(name: Div, duration: &str, status: &str, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .min_h(px(40.))
        .px(px(16.))
        .border_b_1()
        .border_color(rgb(p.edge))
        .child(div().flex_1().min_w_0().child(name))
        .child(div().w(px(65.)).child(duration.to_owned()))
        .child(div().w(px(110.)).truncate().child(status.to_owned()))
}
fn label(s: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(s.into())
}
fn input(cx: &App, f: &Field, id: &'static str, hint: &'static str) -> impl IntoElement {
    let p = *cx.global::<Palette>();
    text_input(id)
        .state(f.downgrade())
        .placeholder(hint)
        .caret_blink_interval_500ms()
        .px(px(9.))
        .py(px(6.))
        .min_h(px(29.))
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(p.edge))
        .bg(rgb(p.base))
        .text_color(rgb(p.text))
        .placeholder_color(rgb(p.muted))
        .caret_color(rgb_to_hsla(rgb(p.text)))
        .w_full()
}
impl Desk {
    fn meetings_view(&self, cx: &Context<Self>, limit: usize, interactive: bool) -> Div {
        let p = *cx.global::<Palette>();
        let mut list = div()
            .flex()
            .flex_col()
            .child(table_row(div().child("Réunion"), "Durée", "État", p).text_color(rgb(p.muted)));
        if self.library.is_empty() {
            return list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child("Aucune réunion trouvée."),
            );
        }
        for m in self.library.iter().take(limit) {
            let id = m.id;
            let mut caption = m.date.clone();
            if !m.project.is_empty() {
                caption.push_str("  ·  ");
                caption.push_str(&m.project);
            }
            let name = div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().truncate().child(m.title.clone()))
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(caption),
                );
            let line = table_row(name, &time(m.duration_ms as u64), &m.status, p);
            if interactive {
                list = list.child(
                    line.id(("meeting", id as usize))
                        .focusable()
                        .tab_stop(true)
                        .role(Role::Button)
                        .aria_label(format!("Ouvrir {}", m.title))
                        .cursor_pointer()
                        .hover(move |s| s.bg(rgb(p.control)))
                        .focus_visible(move |s| s.bg(rgb(p.selected)))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let Err(e) = s.detail(id) {
                                s.message = e.to_string();
                            }
                            cx.notify();
                        })),
                );
            } else {
                list = list.child(line);
            }
        }
        list
    }
    fn setup_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col().child(
            div()
                .flex()
                .gap_3()
                .child(col().flex_1().child(label("Titre")).child(input(
                    cx,
                    &self.title,
                    "title",
                    "Titre de la réunion",
                )))
                .child(col().flex_1().child(label("Projet")).child(input(
                    cx,
                    &self.project,
                    "project",
                    "Facultatif",
                ))),
        );
        for mic in [false, true] {
            body = body.child(label(if mic { "Microphone" } else { "Applications" }).mt_3());
            let mut found = false;
            for (index, i) in self
                .inputs
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i.source, Source::Microphone { .. }) == mic)
            {
                found = true;
                let src = i.source.clone();
                let pid = match &src {
                    Source::Application { pid, .. } => format!(" · PID {pid}"),
                    _ => String::new(),
                };
                let selected = self.chosen.contains(&src);
                let name = format!("{}{pid}", i.name);
                let p = *cx.global::<Palette>();
                let control = checkbox(name, selected, p)
                    .id(("source", index))
                    .justify_between()
                    .px_2()
                    .min_h(px(36.))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(rgb(p.muted))
                            .child(format!("{:.0} %", i.peak * 100.)),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if s.inputs.get(index).is_some_and(|i| i.source == src)
                            && !s.chosen.remove(&src)
                        {
                            if mic {
                                s.chosen.retain(|v| !matches!(v, Source::Microphone { .. }));
                            }
                            s.chosen.insert(src.clone());
                        }
                        cx.notify();
                    }));
                body = body.child(control);
            }
            if !found {
                body = body.child(if self.scanning {
                    "Détection en cours…"
                } else if mic {
                    "Aucune entrée disponible."
                } else {
                    "Aucune session détectée. Lancez un son dans l’application."
                });
            }
        }
        body.child(toggle("Identifier les intervenants","diarization",self,cx))
            .when(self.settings["diarization"]==true,|d|d.child(toggle("Proposer les voix connues","recognize",self,cx)))
            .child(toggle("Générer le compte rendu à l’arrêt","auto_summary",self,cx))
            .child(div().text_color(rgb(cx.global::<Palette>().muted)).child("Informez les participants avant d’enregistrer. Un navigateur sélectionné inclut ses onglets."))
            .child(action("Démarrer l’enregistrement",true,cx,|s,cx|s.start(cx)))
    }
    fn detail_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col().child(
            workspace::strip(*cx.global::<Palette>()).children(
                ["Résumé", "Transcription", "Intervenants"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        button(
                            t,
                            if self.tab == i as u8 {
                                Button::Selected
                            } else {
                                Button::Quiet
                            },
                            None,
                            false,
                            cx,
                            move |s, _| {
                                s.tab = i as u8;
                                s.edit_id = None;
                                Ok(())
                            },
                        )
                        .flex_1()
                    }),
            ),
        );
        let dir = self.audio_dir(self.id);
        if dir.join("sources.json").is_file() && !dir.join("finalise").is_file() {
            body = body.child(action(
                "Récupérer les pistes lisibles",
                false,
                cx,
                |s, _| s.recover_audio(),
            ));
        }
        if self.metadata {
            return body
                .child(input(cx, &self.title, "rename", "Titre"))
                .child(input(cx, &self.project, "category", "Projet"))
                .child(input(cx, &self.description, "description", "Description"))
                .child(input(
                    cx,
                    &self.tags,
                    "tags",
                    "Tags séparés par des virgules",
                ))
                .child(action("Enregistrer les informations", true, cx, |s, cx| {
                    s.db.update_metadata(
                        s.id,
                        s.title.read(cx).as_str(),
                        s.project.read(cx).as_str(),
                        s.description.read(cx).as_str(),
                        s.tags.read(cx).as_str(),
                    )?;
                    s.detail(s.id)
                }));
        }
        match self.tab {
            0 => {
                body = body.child(if self.summary.is_empty() {
                    "Aucun résumé enregistré.".to_owned()
                } else {
                    self.summary.clone()
                });
                if !self.summary.is_empty() {
                    body = body.child(action("Modifier le résumé", false, cx, |s, cx| {
                        s.edit.update(cx, |f, cx| f.emplace(&s.summary, cx));
                        s.edit_id = Some(-1);
                        Ok(())
                    }));
                    if self.edit_id == Some(-1) {
                        body = body
                            .child(
                                text_area("summary-edit")
                                    .state(self.edit.downgrade())
                                    .bg(rgb(cx.global::<Palette>().base))
                                    .rounded(px(9.))
                                    .p_3()
                                    .text_color(rgb(cx.global::<Palette>().text))
                                    .caret_color(rgb_to_hsla(rgb(cx.global::<Palette>().text)))
                                    .w_full()
                                    .h(px(200.)),
                            )
                            .child(action(
                                "Enregistrer une nouvelle version",
                                true,
                                cx,
                                |s, cx| {
                                    s.db.save_summary(
                                        s.id,
                                        s.edit.read(cx).as_str(),
                                        "correction manuelle",
                                        &[],
                                    )?;
                                    s.detail(s.id)
                                },
                            ));
                    }
                    if let Ok(versions) = self.db.versions(self.id)
                        && versions.len() > 1
                    {
                        for (id, body_text) in versions {
                            body = body.child(action(
                                format!("Lire la version {id}"),
                                false,
                                cx,
                                move |s, _| {
                                    s.summary = body_text.clone();
                                    Ok(())
                                },
                            ));
                        }
                    }
                }
                if !self.segments.is_empty() {
                    body = body.child(action("Générer le résumé local", false, cx, |s, _| {
                        s.process(true)
                    }));
                    if self.settings["endpoint"].is_string() {
                        body = body.child(action(
                            "Envoyer le texte au fournisseur…",
                            false,
                            cx,
                            |s, _| s.request_remote_summary(),
                        ));
                    }
                }
            }
            1 => {
                body = body.child(action("Transcrire les pistes", false, cx, |s, _| {
                    s.process(false)
                }));
                if let Ok(versions) = self.db.transcript_versions(self.id)
                    && versions.len() > 1
                {
                    for v in versions {
                        body = body.child(action(
                            format!("Ouvrir la transcription v{v}"),
                            false,
                            cx,
                            move |s, _| {
                                s.segments = s.db.segments_at(s.id, Some(v))?;
                                s.limit = 80;
                                Ok(())
                            },
                        ));
                    }
                }
                for seg in self.segments.iter().take(self.limit) {
                    let id = seg.id;
                    let path = self.audio_dir(self.id).join(&seg.track);
                    let (a, b) = (seg.start_ms, seg.end_ms);
                    body = body.child(action(
                        format!("Écouter §{id}"),
                        false,
                        cx,
                        move |_, _| super::super::playback::play(&path, a, b),
                    ));
                    if self.edit_id == Some(id) {
                        body = body
                            .child(
                                text_area("edit")
                                    .state(self.edit.downgrade())
                                    .bg(rgb(cx.global::<Palette>().base))
                                    .rounded(px(9.))
                                    .p_3()
                                    .text_color(rgb(cx.global::<Palette>().text))
                                    .caret_color(rgb_to_hsla(rgb(cx.global::<Palette>().text)))
                                    .w_full()
                                    .min_h(px(90.))
                                    .border_1()
                                    .p_2(),
                            )
                            .child(action(
                                "Enregistrer la correction",
                                true,
                                cx,
                                move |s, cx| {
                                    s.db.edit(id, s.edit.read(cx).as_str())?;
                                    s.detail(s.id)
                                },
                            ));
                    } else {
                        body = body.child(
                            div()
                                .id(("segment", id as usize))
                                .py_2()
                                .cursor_pointer()
                                .child(label(format!(
                                    "{}    {}    §{}",
                                    time(seg.start_ms as u64),
                                    seg.speaker,
                                    id
                                )))
                                .child(seg.text.clone())
                                .when(seg.echo_candidate.is_some(), |d| {
                                    d.child("Reprise acoustique possible")
                                })
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    if let Some(seg) = s.segments.iter().find(|v| v.id == id) {
                                        s.edit.update(cx, |v, cx| v.emplace(&seg.text, cx));
                                        s.edit_id = Some(id);
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                }
            }
            _ => {
                let mut people = std::collections::BTreeMap::new();
                for seg in &self.segments {
                    *people
                        .entry((&seg.acoustic_label, &seg.speaker))
                        .or_insert(0) += 1;
                }
                if people.is_empty() {
                    body = body.child("Aucune intervention transcrite.");
                }
                for ((raw, name), count) in people {
                    body = body.child(label(format!("{name} · {count} passages")));
                    if raw.starts_with("Intervenant ") && raw != "Intervenant incertain" {
                        let acoustic = raw.to_owned();
                        body = body.child(action(
                            format!("Renommer {raw} pour cette réunion"),
                            false,
                            cx,
                            move |s, cx| {
                                s.db.name_speaker(
                                    s.id,
                                    &acoustic,
                                    s.profile_name.read(cx).as_str(),
                                )?;
                                s.detail(s.id)
                            },
                        ));
                        if let Some(best) = self
                            .segments
                            .iter()
                            .filter(|p| p.acoustic_label == *raw && p.end_ms - p.start_ms >= 1500)
                            .max_by_key(|p| p.end_ms - p.start_ms)
                        {
                            let seg = best.clone();
                            body = body.child(action(
                                format!(
                                    "Créer un profil avec §{} · {}",
                                    seg.id,
                                    time(seg.start_ms as u64)
                                ),
                                false,
                                cx,
                                move |s, cx| {
                                    s.enroll(
                                        seg.clone(),
                                        s.profile_name.read(cx).as_str().to_owned(),
                                    )
                                },
                            ));
                        }
                    }
                }
                body = body.child(input(
                    cx,
                    &self.profile_name,
                    "profile-name",
                    "Nom de l’intervenant ou du profil autorisé",
                ));
                for (raw, p, name, score) in &self.suggestions {
                    body = body.child(format!(
                        "{raw} · proposition {name} · similarité {score:.2}"
                    ));
                    for (txt, choice) in [("Confirmer", Some(*p)), ("Refuser", None)] {
                        let raw = raw.clone();
                        body = body.child(action(
                            format!("{txt} · {raw}"),
                            false,
                            cx,
                            move |s, _| {
                                s.db.confirm(s.id, &raw, choice)?;
                                s.detail(s.id)
                            },
                        ));
                    }
                }
            }
        }
        if self.tab == 1 && self.segments.len() > self.limit {
            body = body.child(action("Afficher la suite", false, cx, |s, _| {
                s.limit += 80;
                Ok(())
            }));
        }
        body = body.child(action("Modifier les informations", false, cx, |s, cx| {
            let (t, p, d, tags) = s.db.metadata(s.id)?;
            for (f, v) in [
                (&s.title, t),
                (&s.project, p),
                (&s.description, d),
                (&s.tags, tags),
            ] {
                f.update(cx, |f, cx| f.emplace(&v, cx));
            }
            s.metadata = true;
            Ok(())
        }));
        for (title, all) in [
            ("Supprimer les fichiers audio", false),
            ("Supprimer la réunion", true),
        ] {
            body = body.child(action(title, false, cx, move |s, _| {
                let id = s.id;
                s.ask(
                    title,
                    if all {
                        "La réunion, sa transcription et ses pistes seront supprimées."
                    } else {
                        "Les pistes audio de cette réunion seront supprimées."
                    },
                    "Supprimer",
                    move |s| {
                        s.delete(id, all)?;
                        if all {
                            s.go(Page::Library)?;
                        } else {
                            s.message = "Fichiers audio supprimés.".into();
                        }
                        Ok(())
                    },
                )
            }));
        }
        body.child(action(
            if self.summary_only {
                "Export · résumé seul"
            } else {
                "Export · résumé et transcription active"
            },
            false,
            cx,
            |s, _| {
                s.summary_only = !s.summary_only;
                Ok(())
            },
        ))
        .child(row().children(["md", "txt", "json"].into_iter().map(|fmt| {
            window_button(
                format!("Exporter .{fmt}"),
                Button::Secondary,
                None,
                false,
                cx,
                move |s, w, cx| s.export(fmt, w, cx),
            )
        })))
    }
    fn preference(&self, pref: Preference, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let (name, key, choices, current) = match pref {
            Preference::Theme => (
                "Thème",
                "light",
                ["Sombre", "Clair"],
                usize::from(self.settings["light"] == true),
            ),
            Preference::Device => (
                "Calcul",
                "device",
                ["CPU", "CUDA"],
                usize::from(self.settings["device"] == "cuda"),
            ),
            Preference::Language => (
                "Langue",
                "language",
                ["Français", "Automatique"],
                usize::from(self.settings["language"] != "fr"),
            ),
        };
        let open = self.menu == Some(pref);
        let field = action(choices[current], false, cx, move |s, _| {
            s.menu = if open { None } else { Some(pref) };
            Ok(())
        })
        .id(SharedString::from(format!("pref-{name}")))
        .aria_label(format!("{name} · {}", choices[current]))
        .justify_between()
        .child(glyph(CHEVRON, 14., p.muted));
        let mut anchor = div().relative().child(field.w_full());
        if open {
            let mut menu = col()
                .gap(px(2.))
                .p(px(3.))
                .bg(rgb(p.raised))
                .rounded(px(10.))
                .border_1()
                .border_color(rgb(p.edge));
            for (index, choice) in choices.into_iter().enumerate() {
                let selected = current == index;
                let item = button(
                    choice,
                    if selected {
                        Button::Selected
                    } else {
                        Button::Quiet
                    },
                    None,
                    false,
                    cx,
                    move |s, cx| {
                        s.menu = None;
                        s.settings[key] = match pref {
                            Preference::Theme => json!(index == 1),
                            Preference::Device => json!(if index == 1 { "cuda" } else { "cpu" }),
                            Preference::Language => json!(if index == 1 { "" } else { "fr" }),
                        };
                        if pref == Preference::Theme {
                            cx.set_global(Palette::new(index == 1));
                        }
                        s.save_settings()
                    },
                )
                .id((name, index))
                .justify_between()
                .when(selected, |d| d.child(glyph(CHECK, 14., p.text)));
                menu = menu.child(item);
            }
            let popover = menu
                .id(SharedString::from(format!("menu-{name}")))
                .occlude()
                .shadow_sm()
                .on_mouse_down_out(cx.listener(move |s, _, _, cx| {
                    if s.menu == Some(pref) {
                        s.menu = None;
                        cx.notify();
                    }
                }));
            anchor = anchor.child(
                div()
                    .absolute()
                    .top_full()
                    .left_0()
                    .right_0()
                    .pt(px(3.))
                    .child(deferred(popover)),
            );
        }
        col().flex_1().gap(px(5.)).child(label(name)).child(anchor)
    }

    fn settings_view(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut presets = col().gap(px(2.)).child(label("Modèle de transcription"));
        for (preset, model) in Page::MODEL_PRESETS
            .into_iter()
            .zip(["tiny", "small", "medium"])
        {
            let recommended = preset == self.recommended;
            let repo = format!("Systran/faster-whisper-{model}");
            let selected = self.settings["whisper_repo"].as_str() == Some(repo.as_str());
            presets = presets.child(
                button(preset, Button::Quiet, None, false, cx, move |s, _| {
                    tracing::info!(preset, "Sélection du modèle");
                    s.menu = None;
                    s.page = Page::ConfirmDownload(preset);
                    Ok(())
                })
                .justify_between()
                .min_h(px(35.))
                .text_color(rgb(p.text))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .text_color(rgb(p.muted))
                        .when(selected, |d| d.child(glyph(CHECK, 14., p.text)))
                        .child(if recommended {
                            format!("{model} · conseillé")
                        } else {
                            model.to_owned()
                        }),
                ),
            );
        }
        let mut local = col().child(label("Modèles déjà téléchargés"));
        for (key, name, folder) in [
            ("whisper", "Dossier Faster-Whisper", true),
            ("segmentation", "Segmentation ONNX", false),
            ("embedding", "Empreintes ONNX", false),
            ("gguf", "Résumé GGUF", false),
        ] {
            local = local
                .child(
                    window_button(name, Button::Secondary, None, false, cx, move |s, w, cx| {
                        s.pick(key, folder, w, cx)
                    })
                    .w_full(),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(
                            self.settings[key]
                                .as_str()
                                .unwrap_or("Aucun modèle sélectionné")
                                .to_owned(),
                        ),
                );
        }
        let audio = col()
            .child(label("Nom de la piste microphone"))
            .child(input(cx, &self.self_name, "self-name", "Moi"))
            .child(action("Enregistrer mon nom", false, cx, |s, cx| {
                let name = s.self_name.read(cx).as_str().trim();
                anyhow::ensure!(!name.is_empty(), "Indiquez un nom.");
                s.settings["self_name"] = json!(name);
                s.save_settings()
            }));
        let provider = col().child(label("Fournisseur distant facultatif"))
            .child(input(cx,&self.endpoint,"endpoint","URL HTTPS du fournisseur"))
            .child(input(cx,&self.remote_model,"remote_model","Nom du modèle distant"))
            .child(div().text_color(rgb(p.muted)).child("Dans le Gestionnaire d’identifiants Windows, créez un identifiant générique MeetingNotes/hôte avec votre clé comme mot de passe."))
            .child(action("Enregistrer le fournisseur",false,cx,|s,cx|{
                let url=s.endpoint.read(cx).as_str().trim();
                anyhow::ensure!(url.is_empty() || (url.starts_with("https://") && !url.contains(['@','?','#'])),"URL HTTPS sans identifiants requise.");
                s.settings["endpoint"]=if url.is_empty(){Value::Null}else{json!(url)};
                s.settings["remote_model"]=json!(s.remote_model.read(cx).as_str());s.save_settings()
            }));
        col()
            .gap(px(20.))
            .child(presets)
            .child(
                col()
                    .child(label("Apparence"))
                    .child(self.preference(Preference::Theme, cx)),
            )
            .child(
                col().child(label("Transcription")).child(
                    div()
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(self.preference(Preference::Device, cx))
                        .child(self.preference(Preference::Language, cx)),
                ),
            )
            .child(local)
            .child(audio)
            .child(provider)
            .child(
                col()
                    .child(label("Cet ordinateur"))
                    .child(self.hardware.clone()),
            )
            .child(
                col()
                    .child(label("Dossier des réunions"))
                    .child(self.root.display().to_string()),
            )
    }
}
fn checkbox(text: String, on: bool, p: Palette) -> Stateful<Div> {
    div()
        .id(SharedString::from(text.clone()))
        .role(Role::CheckBox)
        .aria_label(text.clone())
        .aria_toggled(on.into())
        .focusable()
        .tab_stop(true)
        .flex()
        .items_center()
        .gap_2()
        .min_h(px(28.))
        .rounded(px(6.))
        .cursor_pointer()
        .focus_visible(move |d| d.bg(rgb(p.selected)))
        .child(
            div()
                .size(px(16.))
                .border_1()
                .border_color(rgb(p.muted))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(if on { p.primary } else { p.base }))
                .when(on, |d| d.child(glyph(CHECK, 13., p.on_primary))),
        )
        .child(text)
}

fn toggle(text: &'static str, key: &'static str, s: &Desk, cx: &Context<Desk>) -> impl IntoElement {
    let on = s.settings[key] == true;
    checkbox(text.to_owned(), on, *cx.global::<Palette>())
        .id(key)
        .on_click(cx.listener(move |s, _, _, cx| {
            s.settings[key] = json!(!on);
            if let Err(e) = s.save_settings() {
                s.message = e.to_string();
            }
            cx.notify();
        }))
}

impl Desk {
    fn page_body(&self, page: Page, show_activity: bool, cx: &Context<Self>) -> (String, Div) {
        let (title, body) = if let Some(ctl) = self.ctl.as_ref().filter(|_| show_activity) {
            let mut info = col()
                .child(div().text_3xl().child(time(ctl.elapsed_ms())))
                .child(label(if self.stopping {
                    "Finalisation des pistes…"
                } else if ctl.ready() < self.capture_names.len() {
                    "Ouverture des sources…"
                } else if self.paused {
                    "En pause"
                } else {
                    "● REC"
                }));
            for (i, name) in self.capture_names.iter().enumerate() {
                let level = ctl
                    .levels()
                    .get(&format!("piste-{}.wav", i + 1))
                    .copied()
                    .unwrap_or(0.);
                info = info
                    .child(label(format!("{name}    {:.0} %", level * 100.)))
                    .child(
                        div()
                            .h(px(3.))
                            .w(px(level.clamp(0., 1.) * 220.))
                            .bg(rgb(cx.global::<Palette>().level)),
                    );
            }
            for notice in ctl.incidents().into_values() {
                info = info.child(notice);
            }
            if !self.stopping {
                info = info.child(action("Annuler la réunion", false, cx, |s, _| {
                    s.ask(
                        "Abandonner la réunion ?",
                        "Les pistes de cet enregistrement seront supprimées.",
                        "Abandonner",
                        |s| {
                            s.discard = true;
                            if let Some(c) = &s.ctl {
                                c.stop();
                                s.stopping = true;
                            }
                            Ok(())
                        },
                    )
                }));
                info = info.child(
                    row()
                        .child(action(
                            if self.paused { "Reprendre" } else { "Pause" },
                            false,
                            cx,
                            |s, _| {
                                if let Some(c) = &s.ctl {
                                    if s.paused {
                                        c.resume()
                                    } else {
                                        c.pause()
                                    }
                                    s.paused = !s.paused;
                                }
                                Ok(())
                            },
                        ))
                        .child(action("Arrêter", true, cx, |s, _| {
                            if let Some(c) = &s.ctl {
                                c.stop();
                                s.stopping = true;
                            }
                            Ok(())
                        })),
                );
            }
            (self.title.read(cx).as_str().to_string(), info)
        } else if show_activity && self.cancel.is_some() {
            (
                "Traitement".into(),
                col().child(label(self.stage.clone())).child(action(
                    "Annuler le traitement",
                    false,
                    cx,
                    |s, _| {
                        if let Some(c) = &s.cancel {
                            c.store(true, Ordering::Release);
                        }
                        Ok(())
                    },
                )),
            )
        } else {
            match page {
                Page::Home => ("Espace de travail".into(), self.preview(cx)),
                Page::New => ("Nouvelle réunion".into(), self.setup_view(cx)),
                Page::Library => {
                    let mut d = col()
                        .child(input(cx,
                            &self.search,
                            "search",
                            "Titre, paroles ou intervenant",
                        ))
                        .child(input(cx, &self.filter, "filter", "Projet ou tag"))
                        .child(input(cx, &self.day, "day", "Date · AAAA-MM ou AAAA-MM-JJ"))
                        .child(action("Rechercher", false, cx, |s, cx| s.search_db(cx)));
                    for h in &self.hits {
                        let (id, seg) = (h.meeting, h.segment);
                        d = d.child(action(
                            format!("{} · §{} · {}", time(h.start_ms as u64), seg, h.text),
                            false,
                            cx,
                            move |s, cx| {
                                s.detail(id)?;
                                s.tab = 1;
                                if let Some(p) = s.segments.iter().find(|p| p.id == seg).cloned() {
                                    s.segments = vec![p.clone()];
                                    s.edit.update(cx, |v, cx| v.emplace(&p.text, cx));
                                }
                                Ok(())
                            },
                        ));
                    }
                    (
                        "Mes réunions".into(),
                        d.child(self.meetings_view(cx, usize::MAX, true)),
                    )
                }
                Page::Detail => (
                    self.library
                        .iter()
                        .find(|m| m.id == self.id)
                        .map(|m| m.title.clone())
                        .unwrap_or_default(),
                    self.detail_view(cx),
                ),
                Page::Profiles => {
                    let mut d = col().child(input(cx,
                        &self.profile_name,
                        "new-name",
                        "Nouveau nom du profil",
                    ));
                    if self.profiles.is_empty() {
                        d=d.child("Aucun profil vocal enregistré. Créez un profil depuis une intervention transcrite, avec l’accord de la personne.");
                    }
                    for profile in &self.profiles {
                        let id = profile["id"].as_i64().unwrap_or_default();
                        let name = profile["name"].as_str().unwrap_or("");
                        let entry = div().flex().items_center().gap_2().min_h(px(40.))
                            .border_b_1().border_color(rgb(cx.global::<Palette>().edge))
                            .child(div().flex_1().child(name.to_owned()))
                            .child(action("Renommer", false, cx, move |s,cx| {
                                s.db.rename_profile(id,s.profile_name.read(cx).as_str())?;
                                s.profiles = s.db.profiles(false)?;
                                Ok(())
                            }).id(("rename-profile",id as usize)).aria_label(format!("Renommer {name}")))
                            .child(action("Supprimer",false,cx,move |s,_| {
                                s.ask("Supprimer ce profil vocal ?", "L’extrait autorisé, son empreinte et les suggestions associées seront supprimés.", "Supprimer", move |s| {
                                    s.db.delete_profile(id)?;
                                    s.profiles = s.db.profiles(false)?;
                                    Ok(())
                                })
                            }).id(("delete-profile",id as usize)).aria_label(format!("Supprimer {name}")));
                        d = d.child(entry);
                    }
                    ("Intervenants".into(), d)
                }
                Page::Settings => ("Paramètres".into(), self.settings_view(cx)),
                Page::ConfirmDownload(preset) => (
                    format!("Télécharger le modèle « {preset} » ?"),
                    col()
                        .child("Le modèle de transcription sera téléchargé depuis Hugging Face et conservé sur cet ordinateur.")
                        .child(row()
                            .child(action("Annuler", false, cx, |s, _| s.answer_download(false)))
                            .child(action("Télécharger", true, cx, |s, _| s.answer_download(true)))),
                ),
            }
        };
        (title, body)
    }
}

#[path = "workspace.rs"]
mod workspace;
