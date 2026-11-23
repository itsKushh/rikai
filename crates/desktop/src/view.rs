use super::*;
use gpui::Div;

fn action(
    label: impl Into<String>,
    primary: bool,
    cx: &Context<Desk>,
    f: impl Fn(&mut Desk, &mut Context<Desk>) -> Result<()> + 'static,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(SharedString::from(label.clone()))
        .rounded_md()
        .px_3()
        .py_2()
        .text_sm()
        .role(gpui::Role::Button)
        .tab_stop(true)
        .cursor_pointer()
        .bg(rgb(if primary { 0xe3e7df } else { 0x303035 }))
        .text_color(rgb(if primary { 0x242824 } else { 0xededf0 }))
        .hover(|s| s.opacity(0.84))
        .active(|s| s.opacity(0.70))
        .child(label)
        .on_click(cx.listener(move |s, _, _, cx| {
            s.message.clear();
            if let Err(e) = f(s, cx) {
                s.message = format!("{e:#}");
            }
            cx.notify();
        }))
}
fn col() -> Div {
    div().flex().flex_col().gap_3()
}
fn row() -> Div {
    div().flex().flex_wrap().items_center().gap_2()
}
fn label(s: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(s.into())
}
fn input(f: &Field, id: &'static str, hint: &'static str) -> impl IntoElement {
    text_input(id)
        .state(f.downgrade())
        .placeholder(hint)
        .caret_blink_interval_500ms()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x77777c))
        .w_full()
}
impl Desk {
    fn meetings_view(&self, cx: &Context<Self>, limit: usize) -> Div {
        let mut list = col();
        if self.library.is_empty() {
            return list.child("Aucune réunion trouvée.");
        }
        for m in self.library.iter().take(limit) {
            let id = m.id;
            list = list.child(
                div()
                    .id(("meeting", id as usize))
                    .cursor_pointer()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(0x454549))
                    .child(label(m.title.clone()))
                    .child(div().text_sm().child(format!(
                        "{}    ·    {}    ·    {}    {}",
                        m.date,
                        time(m.duration_ms as u64),
                        m.status,
                        m.project
                    )))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Err(e) = s.detail(id) {
                            s.message = e.to_string();
                        }
                        cx.notify();
                    })),
            );
        }
        list
    }
    fn setup_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col()
            .child(label("Titre"))
            .child(input(&self.title, "title", "Titre de la réunion"))
            .child(label("Projet"))
            .child(input(&self.project, "project", "Facultatif"));
        for mic in [false, true] {
            body = body.child(label(if mic { "Microphone" } else { "Applications" }));
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
                body = body.child(action(
                    format!(
                        "{}  {}{}   ·   {:.0} %",
                        if self.chosen.contains(&src) {
                            "☑"
                        } else {
                            "☐"
                        },
                        i.name,
                        pid,
                        i.peak * 100.
                    ),
                    false,
                    cx,
                    move |s, _| {
                        if s.inputs.get(index).is_some_and(|i| i.source == src)
                            && !s.chosen.remove(&src)
                        {
                            if mic {
                                s.chosen.retain(|v| !matches!(v, Source::Microphone { .. }));
                            }
                            s.chosen.insert(src.clone());
                        }
                        Ok(())
                    },
                ));
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
            .child(div().text_sm().child("Informez les participants avant d’enregistrer. Un navigateur sélectionné inclut ses onglets."))
            .child(action("Démarrer l’enregistrement",true,cx,|s,cx|s.start(cx)))
    }
    fn detail_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col().child(
            row().children(
                ["Résumé", "Transcription", "Intervenants"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        action(t, self.tab == i as u8, cx, move |s, _| {
                            s.tab = i as u8;
                            s.edit_id = None;
                            Ok(())
                        })
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
                .child(input(&self.title, "rename", "Titre"))
                .child(input(&self.project, "category", "Projet"))
                .child(input(&self.description, "description", "Description"))
                .child(input(&self.tags, "tags", "Tags séparés par des virgules"))
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
                    if let Ok(versions) = self.db.versions(self.id) {
                        if versions.len() > 1 {
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
                            |s, _| s.process_via(true, true),
                        ));
                    }
                }
            }
            1 => {
                body = body.child(action("Transcrire les pistes", false, cx, |s, _| {
                    s.process(false)
                }));
                if let Ok(versions) = self.db.transcript_versions(self.id) {
                    if versions.len() > 1 {
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
                if confirm(title) {
                    s.delete(s.id, all)?;
                    if all {
                        s.go(Page::Library)?;
                    } else {
                        s.message = "Fichiers audio supprimés.".into();
                    }
                }
                Ok(())
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
            action(format!("Exporter .{fmt}"), false, cx, move |s, _| {
                s.export(fmt)
            })
        })))
    }
    fn settings_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col()
            .child(label("Cet ordinateur"))
            .child(self.hardware.clone())
            .child(format!("Profil CPU conseillé : {}", self.recommended))
            .child(label("Modèles locaux"))
            .child("Sélectionnez des modèles déjà téléchargés sur cet ordinateur.");
        body = body
            .child(label("Télécharger un modèle de transcription"))
            .child(
                row().children(Page::MODEL_PRESETS.into_iter().map(|preset| {
                    action(preset, false, cx, move |s, _| {
                        tracing::info!(preset, "Sélection du modèle");
                        s.page = Page::ConfirmDownload(preset);
                        Ok(())
                    })
                })),
            );
        for (key, name, folder) in [
            ("whisper", "Transcription · dossier Faster-Whisper", true),
            ("segmentation", "Diarisation · segmentation ONNX", false),
            ("embedding", "Diarisation · empreintes ONNX", false),
            ("gguf", "Résumé · fichier GGUF", false),
        ] {
            body = body
                .child(action(name, false, cx, move |s, _| s.pick(key, folder)))
                .child(
                    div().text_sm().child(
                        self.settings[key]
                            .as_str()
                            .unwrap_or("Aucun modèle sélectionné")
                            .to_string(),
                    ),
                );
        }
        body.child(action(format!("Calcul · {}",self.settings["device"].as_str().unwrap_or("cpu")),false,cx,|s,_|{s.settings["device"]=json!(if s.settings["device"]=="cuda"{"cpu"}else{"cuda"});s.save_settings()}))
            .child(action(format!("Langue · {}",if self.settings["language"]=="fr"{"Français"}else{"Automatique"}),false,cx,|s,_|{s.settings["language"]=json!(if s.settings["language"]=="fr"{""}else{"fr"});s.save_settings()}))
            .child(label("Nom de la piste microphone")).child(input(&self.self_name,"self-name","Moi")).child(action("Enregistrer mon nom",false,cx,|s,cx|{let name=s.self_name.read(cx).as_str().trim();anyhow::ensure!(!name.is_empty(),"Indiquez un nom.");s.settings["self_name"]=json!(name);s.save_settings()}))
            .child(label("Fournisseur distant facultatif")).child(input(&self.endpoint,"endpoint","URL de base HTTPS, par exemple https://exemple.fr/v1")).child(input(&self.remote_model,"remote_model","Nom du modèle distant"))
            .child(div().text_sm().child("Dans le Gestionnaire d’identifiants Windows, créez un identifiant générique MeetingNotes/hôte avec votre clé comme mot de passe."))
            .child(action("Enregistrer le fournisseur",false,cx,|s,cx|{let url=s.endpoint.read(cx).as_str().trim();
                anyhow::ensure!(url.is_empty() || (url.starts_with("https://") && !url.contains(['@','?','#'])),"URL HTTPS sans identifiants requise.");
                s.settings["endpoint"]=if url.is_empty(){Value::Null}else{json!(url)};s.settings["remote_model"]=json!(s.remote_model.read(cx).as_str());s.save_settings()}))
            .child(toggle("Thème clair","light",self,cx)).child(label("Dossier des réunions")).child(self.root.display().to_string())
    }
}
fn toggle(text: &'static str, key: &'static str, s: &Desk, cx: &Context<Desk>) -> impl IntoElement {
    action(
        format!(
            "{}  {text}",
            if s.settings[key] == true {
                "☑"
            } else {
                "☐"
            }
        ),
        false,
        cx,
        move |s, _| {
            s.settings[key] = json!(s.settings[key] != true);
            s.save_settings()
        },
    )
}
impl Render for Desk {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let light = self.settings["light"] == true;
        let mut side = col()
            .w(px(174.))
            .flex_shrink_0()
            .p_4()
            .bg(rgb(if light { 0xe6e6e7 } else { 0x19191c }))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb_5()
                    .child("MeetingNotes"),
            );
        if !self.busy() {
            for (p, t) in [
                (Page::Home, "Accueil"),
                (Page::New, "Nouvelle réunion"),
                (Page::Library, "Mes réunions"),
                (Page::Profiles, "Intervenants"),
                (Page::Settings, "Paramètres"),
            ] {
                side = side.child(action(t, self.page == p, cx, move |s, _| s.go(p)));
            }
        }
        let (title, body) = if let Some(ctl) = &self.ctl {
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
                            .bg(rgb(0x78ad88)),
                    );
            }
            for notice in ctl.incidents().into_values() {
                info = info.child(notice);
            }
            if !self.stopping {
                info = info.child(action("Annuler la réunion", false, cx, |s, _| {
                    if confirm("Abandonner la réunion et supprimer ses pistes ?") {
                        s.discard = true;
                        if let Some(c) = &s.ctl {
                            c.stop();
                            s.stopping = true;
                        }
                    }
                    Ok(())
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
        } else if self.cancel.is_some() {
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
            match self.page {
                Page::Home => {
                    (
                        "Accueil".into(),
                        col()
                            .child(action("Créer une réunion", true, cx, |s, _| {
                                s.go(Page::New)
                            }))
                            .child(label("Réunions récentes"))
                            .child(self.meetings_view(cx, 5))
                            .child(div().text_sm().child(
                                if self.settings["whisper"].is_string() {
                                    "Modèle de transcription sélectionné"
                                } else {
                                    "Modèle de transcription à configurer dans Paramètres"
                                },
                            )),
                    )
                }
                Page::New => ("Nouvelle réunion".into(), self.setup_view(cx)),
                Page::Library => {
                    let mut d = col()
                        .child(input(
                            &self.search,
                            "search",
                            "Titre, paroles ou intervenant",
                        ))
                        .child(input(&self.filter, "filter", "Projet ou tag"))
                        .child(input(&self.day, "day", "Date · AAAA-MM ou AAAA-MM-JJ"))
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
                        d.child(self.meetings_view(cx, usize::MAX)),
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
                    let mut d = col().child(input(
                        &self.profile_name,
                        "new-name",
                        "Nouveau nom du profil",
                    ));
                    if self.profiles.is_empty() {
                        d=d.child("Aucun profil vocal enregistré. Créez un profil depuis une intervention transcrite, avec l’accord de la personne.");
                    }
                    for p in &self.profiles {
                        let id = p["id"].as_i64().unwrap_or_default();
                        let n = p["name"].as_str().unwrap_or("");
                        d=d.child(row().child(label(n.to_owned())).child(action(format!("Renommer le profil {id}"),false,cx,move|s,cx|{s.db.rename_profile(id,s.profile_name.read(cx).as_str())?;s.profiles=s.db.profiles(false)?;Ok(())})).child(action(format!("Supprimer le profil {id}"),false,cx,move|s,_|{
                        if rfd::MessageDialog::new().set_title("Supprimer ce profil vocal ?").set_description("L’extrait autorisé, son empreinte et les suggestions associées seront supprimés.").set_buttons(rfd::MessageButtons::YesNo).show()==rfd::MessageDialogResult::Yes{s.db.delete_profile(id)?;s.profiles=s.db.profiles(false)?;}Ok(())
                    })));
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
        div()
            .size_full()
            .flex()
            .font_family("Inter")
            .bg(rgb(if light { 0xf6f6f7 } else { 0x111113 }))
            .text_color(rgb(if light { 0x222226 } else { 0xededf0 }))
            .child(side)
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .p_6()
                    .child(
                        col()
                            .max_w(px(760.))
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .mb_2()
                                    .child(title),
                            )
                            .when(!self.message.is_empty(), |s| {
                                s.child(
                                    div()
                                        .p_3()
                                        .border_1()
                                        .rounded_md()
                                        .child(self.message.clone()),
                                )
                            })
                            .child(body),
                    ),
            )
    }
}

pub(super) fn confirm(title: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title("MeetingNotes")
        .set_description(title)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}
