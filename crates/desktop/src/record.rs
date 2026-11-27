use super::*;

impl Desk {
    pub(super) fn record_header(&self, title: String, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        div()
            .flex()
            .items_center()
            .gap_2()
            .min_h(px(43.))
            .px_4()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(p.edge))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(self.record_menu_button(RecordMenu::Export, "Exporter", SAVE, cx))
            .child(self.record_menu_button(
                RecordMenu::More,
                "Options de l’enregistrement",
                MORE,
                cx,
            ))
    }

    fn record_menu_button(
        &self,
        menu: RecordMenu,
        name: &'static str,
        icon: &'static str,
        cx: &Context<Self>,
    ) -> Div {
        let open = self.record_menu == Some(menu);
        let p = *cx.global::<Palette>();
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measured = bounds.clone();
        let mut anchor = div().relative().flex_shrink_0().child(
            button(
                name,
                if open {
                    Button::Selected
                } else {
                    Button::Quiet
                },
                Some(icon),
                true,
                cx,
                move |s, _| {
                    s.record_menu = if s.record_menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    Ok(())
                },
            )
            .on_prepaint(move |hit, _, _| measured.set(hit.bounds))
            .aria_expanded(open)
            .size(px(29.))
            .p_0(),
        );
        if open {
            let options = match menu {
                RecordMenu::History => self.record_history(cx),
                RecordMenu::More => self.record_options(cx),
                RecordMenu::Export => self.export_options(cx),
            };
            let popup = col()
                .id("record-action-menu")
                .occlude()
                .p(px(6.))
                .gap(px(2.))
                .w(px(264.))
                .max_h(px(220.))
                .rounded(px(16.))
                .border_1()
                .border_color(rgb(p.edge))
                .bg(rgb(p.raised))
                .shadow(p.bevel())
                .on_mouse_down_out(cx.listener(move |s, event: &MouseDownEvent, _, cx| {
                    // The trigger handles its own click; don't re-open the menu on mouse up.
                    if bounds.get().contains(&event.position) {
                        return;
                    }
                    if s.record_menu == Some(menu) {
                        s.record_menu = None;
                        cx.notify();
                    }
                }))
                .child(
                    super::scroller::ScrollArea::new("record-action-options", options)
                        .limited(206.),
                )
                .with_animation("record-options-open", reveal(cx, 140), |d, t| {
                    d.relative().top(px(-3. * (1. - t))).opacity(t)
                });
            anchor = anchor.child(
                div().absolute().top_full().right_0().child(deferred(
                    anchored()
                        .anchor(Anchor::TopRight)
                        .snap_to_window_with_margin(px(9.))
                        .child(popup),
                )),
            );
        }
        anchor
    }

    fn record_options(&self, cx: &Context<Self>) -> Div {
        let mut options = col().gap(px(2.)).child(
            button(
                "Modifier les informations",
                Button::Quiet,
                Some(EDIT),
                false,
                cx,
                |s, cx| {
                    s.record_menu = None;
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
                },
            )
            .justify_start(),
        );
        let dir = self.audio_dir(self.id);
        if dir.join("sources.json").is_file() && !dir.join("finalise").is_file() {
            options = options.child(
                button(
                    "Récupérer les pistes lisibles",
                    Button::Quiet,
                    Some(AUDIO),
                    false,
                    cx,
                    |s, _| {
                        s.record_menu = None;
                        s.recover_audio()
                    },
                )
                .justify_start(),
            );
        }
        for (title, all) in [
            ("Supprimer les fichiers audio", false),
            ("Supprimer l’enregistrement", true),
        ] {
            options = options.child(
                button(title, Button::Quiet, None, false, cx, move |s, _| {
                    s.record_menu = None;
                    let id = s.id;
                    s.ask(
                        title,
                        if all {
                            "L’enregistrement, sa transcription et ses pistes seront supprimées."
                        } else {
                            "Les pistes audio de cet enregistrement seront supprimées."
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
                })
                .justify_start(),
            );
        }
        options
    }

    fn export_options(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        col()
            .gap(px(3.))
            .child(
                checkbox(tr("Résumé seul").into(), self.summary_only, false, p)
                    .px_2()
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.summary_only = !s.summary_only;
                        cx.notify();
                    })),
            )
            .children(
                meeting_storage::export::FORMATS
                    .into_iter()
                    .filter(|(fmt, _, _)| {
                        !self.summary_only || matches!(*fmt, "md" | "txt" | "json")
                    })
                    .map(|(fmt, name, ext)| {
                        window_button(
                            format!("{}  ·  .{ext}", tr(name)),
                            Button::Quiet,
                            None,
                            false,
                            cx,
                            move |s, w, cx| {
                                s.record_menu = None;
                                s.export(fmt, w, cx)
                            },
                        )
                        .justify_start()
                    }),
            )
    }

    fn record_history(&self, cx: &Context<Self>) -> Div {
        let tab = self.tab as usize;
        let ids = if tab == 0 {
            &self.summary_ids
        } else {
            &self.transcript_ids
        };
        col().gap(px(2.)).children(ids.iter().copied().map(|id| {
            let selected = self.reading_version[tab] == Some(id);
            let label = if tab == 0 {
                format!("Lire la version {id}")
            } else {
                format!("Ouvrir la transcription v{id}")
            };
            button(
                label,
                if selected {
                    Button::Selected
                } else {
                    Button::Quiet
                },
                selected.then_some(CHECK),
                false,
                cx,
                move |s, _| {
                    s.record_menu = None;
                    if tab == 0 {
                        s.set_report(s.db.summary_at(s.id, id)?);
                    } else {
                        s.segments = s.db.segments_at(s.id, Some(id))?;
                        s.load_review_queue();
                        s.stop_segment();
                        s.dialogues
                            .reset_with_uniform_height(s.segments.len(), px(99.));
                        s.language_picker = None;
                    }
                    s.reading_version[tab] = Some(id);
                    s.edit_id = None;
                    Ok(())
                },
            )
            .justify_start()
        }))
    }

    fn reader_tools(&self, cx: &Context<Self>) -> Div {
        let summary = self.tab == 0;
        let p = *cx.global::<Palette>();
        let mut tools = div().flex().items_center().gap(px(5.)).flex_shrink_0();
        let mut actions = row().gap(px(5.));
        if summary && !self.segments.is_empty() {
            actions = actions.child(button(
                "Générer le résumé local",
                Button::Secondary,
                Some(MODEL),
                false,
                cx,
                |s, _| s.offer_summary(),
            ));
            if self.settings["endpoint"].is_string() {
                actions = actions.child(
                    button(
                        "Envoyer le texte au fournisseur…",
                        Button::Quiet,
                        Some(REMOTE),
                        true,
                        cx,
                        |s, _| s.request_remote_summary(),
                    )
                    .size(px(29.))
                    .p_0(),
                );
            }
        } else if !summary {
            actions = actions.child(button(
                "Transcrire les pistes",
                Button::Secondary,
                Some(WAVE),
                false,
                cx,
                |s, _| s.process(false),
            ));
        }
        tools = tools.child(actions.flex_1().min_w_0());
        let tab = self.tab as usize;
        let count = if summary {
            self.summary_ids.len()
        } else {
            self.transcript_ids.len()
        };
        if count > 1 {
            if let Some(id) = self.reading_version[tab] {
                tools = tools.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(format!("v{id}")),
                );
            }
            tools = tools.child(self.record_menu_button(
                RecordMenu::History,
                "Historique des versions",
                HISTORY,
                cx,
            ));
        }
        if summary && !self.summary.is_empty() {
            tools = tools.child(
                button(
                    "Modifier le résumé",
                    Button::Quiet,
                    Some(EDIT),
                    true,
                    cx,
                    |s, cx| {
                        s.record_menu = None;
                        s.edit.update(cx, |f, cx| f.emplace(&s.summary, cx));
                        s.edit_id = Some(-1);
                        Ok(())
                    },
                )
                .size(px(29.))
                .p_0(),
            );
        }
        tools
    }

    pub(super) fn transcript_reader(&self, cx: &Context<Self>) -> Div {
        let body = col().flex_1().min_h_0().child(self.reader_tools(cx));
        if self.segments.is_empty() {
            return body.child(tr("Aucune intervention transcrite."));
        }
        body.child(self.dialogue_list(cx))
    }

    pub(super) fn summary_reader(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let body = col().min_h_0().flex_1();
        if self.edit_id == Some(-1) {
            return body
                .child(
                    row()
                        .flex_shrink_0()
                        .child(button(
                            "Enregistrer une nouvelle version",
                            Button::Primary,
                            Some(CHECK),
                            false,
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
                        ))
                        .child(button("Annuler", Button::Quiet, None, false, cx, |s, _| {
                            s.edit_id = None;
                            Ok(())
                        })),
                )
                .child(
                    text_area("summary-edit")
                        .state(self.edit.downgrade())
                        .flex_1()
                        .min_h_0()
                        .bg(rgb(p.base))
                        .rounded(px(11.))
                        .p_3()
                        .text_color(rgb(p.text))
                        .caret_color(rgb_to_hsla(rgb(p.text))),
                );
        }
        let body = body.child(self.reader_tools(cx));
        if self.report_lines.is_empty() {
            return body.child(tr("Aucun résumé enregistré."));
        }
        body.child(
            div()
                .flex()
                .min_h_0()
                .flex_1()
                .child(
                    list(
                        self.report_list.clone(),
                        cx.processor(|s, i: usize, _, _| {
                            let line = &s.report_lines[i];
                            let body = if let Some(title) = line.strip_prefix("## ") {
                                raw_label(title.to_owned()).pb(px(3.))
                            } else if let Some(text) = line.strip_prefix("- ") {
                                div()
                                    .flex()
                                    .gap(px(7.))
                                    .pb(px(3.))
                                    .child("•")
                                    .child(div().min_w_0().flex_1().child(text.to_owned()))
                            } else if line.trim().is_empty() {
                                div().h(px(6.))
                            } else {
                                div().pb(px(3.)).child(line.to_owned())
                            };
                            s.wheel[0]
                                .listen(scroller::Axis::Rows(s.report_list.clone()), body)
                                .into_any_element()
                        }),
                    )
                    .flex_1()
                    .min_w_0()
                    .h_full(),
                )
                .child(self.reading_scrollbar(cx)),
        )
    }
}
