use super::*;

fn duration(ms: i64) -> String {
    let secs = ms.max(0) as f64 / 1000.;
    if secs < 60. {
        format!("{secs:.1} s").replace('.', ",")
    } else {
        format!("{}:{:02}", ms / 60000, (ms / 1000) % 60)
    }
}

impl Desk {
    pub(in crate::ui) fn load_review_queue(&mut self) {
        self.review_queue = self
            .segments
            .iter()
            .enumerate()
            .filter_map(|(i, segment)| meeting_storage::review::flag(segment).map(|flag| (i, flag)))
            .collect();
        self.review_queue.sort_by_key(|(_, flag)| flag.reason);
        self.review_list
            .reset_with_uniform_height(self.review_queue.len(), px(99.));
    }

    fn remove_reviewed(&mut self, ix: usize) {
        self.segments[ix].reviewed = true;
        if let Some(row) = self.review_queue.iter().position(|(i, _)| *i == ix) {
            self.review_queue.remove(row);
            self.review_list.splice(row..row + 1, 0);
        }
    }

    fn remeasure_segment(&self, ix: usize) {
        self.dialogues.remeasure_items(ix..ix + 1);
        if self.tab == REVIEW_TAB
            && let Some(row) = self.review_queue.iter().position(|(i, _)| *i == ix)
        {
            self.review_list.remeasure_items(row..row + 1);
        }
    }

    fn edit_segment(&mut self, ix: usize, id: i64, w: &mut Window, cx: &mut Context<Self>) {
        let Some(seg) = self.segments.get(ix).filter(|s| s.id == id) else {
            return;
        };
        self.edit
            .update(cx, |field, cx| field.emplace(&seg.text, cx));
        self.edit.read(cx).focus_handle(cx).focus(w, cx);
        if let Some(previous) = self
            .edit_id
            .and_then(|old| self.segments.iter().position(|s| s.id == old))
        {
            self.remeasure_segment(previous);
        }
        self.edit_id = Some(id);
        self.remeasure_segment(ix);
        cx.notify();
    }

    pub(super) fn review_reader(&self, cx: &Context<Self>) -> Div {
        let body = col().flex_1().min_h_0();
        if self.review_queue.is_empty() {
            return body.child(tr(if self.segments.is_empty() {
                "Aucune intervention transcrite."
            } else if self.segments.iter().all(|s| {
                !s.reviewed
                    && s.confidence.is_none()
                    && s.words.iter().all(|w| w.confidence.is_none())
            }) {
                "Cette transcription ne contient aucun indice de confiance."
            } else {
                "Aucun passage à vérifier."
            }));
        }
        body.child(
            div()
                .text_size(px(12.))
                .text_color(rgb(cx.global::<Palette>().muted))
                .child(format!(
                    "{} · {}",
                    tr("Passages à vérifier"),
                    self.review_queue.len()
                )),
        )
        .child(self.dialogue_list(cx))
    }

    pub(super) fn dialogue_list(&self, cx: &Context<Self>) -> Div {
        let state = self.reading_list().clone();
        div()
            .flex()
            .min_h_0()
            .flex_1()
            .child(
                list(
                    state,
                    cx.processor(|s, ix, _, cx| {
                        let (segment_ix, flag) = if s.tab == REVIEW_TAB {
                            let (i, flag) = s.review_queue[ix];
                            (i, Some(flag))
                        } else {
                            (ix, None)
                        };
                        s.wheel[1]
                            .listen(
                                scroller::Axis::Rows(s.reading_list().clone()),
                                if s.tab == 4 {
                                    let seg: &Segment = &s.segments[segment_ix];
                                    col()
                                        .py_2()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(cx.global::<Palette>().muted))
                                                .child(format!(
                                                    "{} → {}  ·  {}",
                                                    meeting_storage::export::stamp(
                                                        seg.start_ms,
                                                        '.'
                                                    ),
                                                    meeting_storage::export::stamp(seg.end_ms, '.'),
                                                    seg.speaker
                                                )),
                                        )
                                        .child(seg.text.clone())
                                } else {
                                    s.segment_card(segment_ix, &s.segments[segment_ix], flag, cx)
                                },
                            )
                            .into_any_element()
                    }),
                )
                .flex_1()
                .min_w_0()
                .h_full(),
            )
            .child(self.reading_scrollbar(cx))
    }
    fn segment_card(
        &self,
        ix: usize,
        seg: &Segment,
        flag: Option<meeting_storage::review::Flag>,
        cx: &Context<Self>,
    ) -> Div {
        let p = *cx.global::<Palette>();
        let id = seg.id;
        let playing = self.playing.is_some_and(|v| v.id == id && !v.paused);
        let mut info = col().flex_1().min_w_0().gap(px(4.)).child(
            row()
                .gap(px(8.))
                .child(raw_label(
                    if matches!(seg.speaker.as_str(), "Moi" | "Voix distante") {
                        tr(&seg.speaker).to_owned()
                    } else {
                        seg.speaker.clone()
                    },
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(time(seg.start_ms.max(0) as u64)),
                ),
        );
        if let Some(flag) = flag {
            use meeting_storage::review::Reason;
            let label = tr(match flag.reason {
                Reason::Negation => "Négation incertaine",
                Reason::Quantity => "Quantité incertaine",
                Reason::Wording => "Transcription incertaine",
                Reason::Overlap => "Voix superposées",
                Reason::Speaker => "Intervenant incertain",
                Reason::Echo => "Reprise acoustique possible",
            });
            let detail = match flag.word {
                Some(i) => format!("{label} · « {} »", seg.words[i].text.trim()),
                None => label.to_owned(),
            };
            info = info.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .child(detail),
            );
        } else {
            info = info.child(self.segment_timeline(ix, seg, cx));
        }
        if self.edit_id == Some(id) {
            info = info
                .child(
                    text_area("passage-edit")
                        .state(self.edit.downgrade())
                        .bg(rgb(p.base))
                        .text_color(rgb(p.text))
                        .caret_color(rgb_to_hsla(rgb(p.text)))
                        .shadow(p.bevel())
                        .p_2()
                        .rounded(px(10.))
                        .w_full()
                        .min_h(px(60.)),
                )
                .child(
                    row()
                        .child(action(
                            "Enregistrer la correction",
                            true,
                            cx,
                            move |s, cx| {
                                let text = s.edit.read(cx).as_str().to_owned();
                                s.db.edit(id, &text)?;
                                s.segments[ix].text = text;
                                s.remove_reviewed(ix);
                                if s.tab == REVIEW_TAB && s.playing.is_some_and(|p| p.id == id) {
                                    s.stop_segment();
                                }
                                s.translations = s.db.translations(s.id)?;
                                s.edit_id = None;
                                s.remeasure_segment(ix);
                                Ok(())
                            },
                        ))
                        .child(button(
                            "Annuler",
                            Button::Quiet,
                            None,
                            false,
                            cx,
                            move |s, _| {
                                s.edit_id = None;
                                s.remeasure_segment(ix);
                                Ok(())
                            },
                        )),
                );
        } else {
            info = info.child(
                div()
                    .id(("segment-text", ix))
                    .role(Role::Button)
                    .focusable()
                    .tab_stop(true)
                    .aria_label(crate::i18n::message(&format!("Corriger le passage {id}")))
                    .cursor_text()
                    .rounded(px(3.))
                    .focus_visible(move |d| d.bg(rgb(p.selected)))
                    .text_size(px(13.))
                    .child(seg.text.clone())
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.edit_segment(ix, id, w, cx);
                    })),
            );
        }
        if flag.is_none() && seg.echo_candidate.is_some() {
            info = info.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .child(tr("Reprise acoustique possible")),
            );
        }
        if flag.is_none()
            && let Some(items) = self.translations.get(&id)
        {
            for (lang, text) in items {
                info = info.child(
                    col()
                        .gap(px(3.))
                        .mt_1()
                        .border_t_1()
                        .border_color(rgb(p.edge))
                        .pt_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(p.muted))
                                .child(crate::i18n::language_name(lang).to_string()),
                        )
                        .child(text.clone()),
                );
            }
        }
        let actions = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .flex_shrink_0()
            .child(
                button(
                    if flag.is_some() {
                        tr(if playing {
                            "Mettre en pause"
                        } else {
                            "Écouter avec le contexte"
                        })
                        .to_owned()
                    } else {
                        format!(
                            "{} le passage §{id}",
                            if playing {
                                "Mettre en pause"
                            } else {
                                "Écouter"
                            }
                        )
                    },
                    Button::Secondary,
                    Some(if playing { PAUSE } else { PLAY }),
                    true,
                    cx,
                    move |s, _| s.listen(id, None),
                )
                .id(("passage-play", ix))
                .size(px(31.))
                .p_0(),
            );
        let actions = if flag.is_some() {
            actions.when(self.edit_id != Some(id), |d| {
                d.child(
                    window_button(
                        format!("Corriger le passage {id}"),
                        Button::Quiet,
                        Some(EDIT),
                        true,
                        cx,
                        move |s, w, cx| {
                            s.edit_segment(ix, id, w, cx);
                            Ok(())
                        },
                    )
                    .id(("review-edit", ix))
                    .size(px(31.))
                    .p_0(),
                )
                .child(
                    button(
                        "Marquer comme vérifié",
                        Button::Quiet,
                        Some(CHECK),
                        true,
                        cx,
                        move |s, _| {
                            s.db.confirm_segment(id)?;
                            if s.playing.is_some_and(|p| p.id == id) {
                                s.stop_segment();
                            }
                            s.remove_reviewed(ix);
                            Ok(())
                        },
                    )
                    .id(("review-confirm", ix))
                    .size(px(31.))
                    .p_0(),
                )
            })
        } else {
            actions
                .child(
                    window_button(
                        format!("Enregistrer le passage §{id} en WAV"),
                        Button::Quiet,
                        Some(SAVE),
                        true,
                        cx,
                        move |s, w, cx| s.save_clip(id, w, cx),
                    )
                    .id(("passage-save", ix))
                    .size(px(31.))
                    .p_0(),
                )
                .child(self.translation_picker(ix, id, cx))
        };
        div().pb(px(8.)).pr(px(3.)).child(
            p.panel()
                .px(px(13.))
                .py(px(10.))
                .flex_row()
                .items_center()
                .gap(px(14.))
                .child(info)
                .child(actions),
        )
    }

    fn segment_timeline(&self, ix: usize, seg: &Segment, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let id = seg.id;
        let length = seg.end_ms - seg.start_ms;
        let current = self.playing.filter(|v| v.id == id);
        let pos = current.map(|v| v.position - seg.start_ms).unwrap_or(0);
        let fraction = pos as f32 / length.max(1) as f32;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measure = bounds.clone();
        let seeker = div()
            .id(("segment-timeline", ix))
            .relative()
            .flex_1()
            .h(px(18.))
            .flex()
            .items_center()
            .role(Role::Slider)
            .aria_label(crate::i18n::message(&format!("Position du passage {id}")))
            .aria_value(duration(pos))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .rounded(px(5.))
            .focus_visible(move |d| d.bg(rgb(p.selected)))
            .on_prepaint(move |b, _, _| measure.set(b.bounds))
            .on_click(cx.listener(move |s, event: &ClickEvent, _, cx| {
                let b = bounds.get();
                let from = if event.is_keyboard() {
                    0.
                } else {
                    (event.position().x - b.origin.x) / b.size.width.max(px(1.))
                };
                if let Err(e) = s.listen(id, Some(from)) {
                    s.message = e.to_string();
                }
                cx.notify();
            }))
            .on_key_down(cx.listener(move |s, e: &KeyDownEvent, _, cx| {
                let step = match e.keystroke.key.as_str() {
                    "left" => -1000,
                    "right" => 1000,
                    _ => return,
                };
                if let Err(e) = s.listen(id, Some((pos + step) as f32 / length.max(1) as f32)) {
                    s.message = e.to_string();
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(bar(fraction, p.primary, p));

        div().flex().items_center().gap_2().child(seeker).child(
            div()
                .text_size(px(11.))
                .text_color(rgb(p.muted))
                .child(duration(length)),
        )
    }

    fn translation_picker(&self, ix: usize, id: i64, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let translate = window_button(
            "Traduire le passage",
            Button::Quiet,
            Some(TRANSLATE),
            true,
            cx,
            move |s, w, cx| {
                if s.language_picker == Some(id) {
                    s.language_picker = None;
                } else {
                    s.language_picker = Some(id);
                    s.language_search.update(cx, |v, cx| v.emplace("", cx));
                    s.language_search.read(cx).focus_handle(cx).focus(w, cx);
                }
                Ok(())
            },
        )
        .id(("passage-translate", ix))
        .size(px(31.))
        .p_0();
        div()
            .relative()
            .child(translate)
            .when(self.language_picker == Some(id), |anchor| {
                let q = self.language_search.read(cx).as_str().trim().to_lowercase();
                let mut rows = col().gap(px(2.));
                let mut found = false;
                for (n, row) in crate::i18n::languages()
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        q.is_empty() || r.iter().any(|s| s.to_lowercase().contains(&q))
                    })
                {
                    found = true;
                    let target = row[0].clone();
                    let native = &row[1];
                    let local = crate::i18n::language_name(&target);
                    let item = if local.eq_ignore_ascii_case(native) {
                        native.clone()
                    } else {
                        format!("{local} · {native}")
                    };
                    rows = rows.child(
                        button(item, Button::Quiet, None, false, cx, move |s, _| {
                            s.start_translation(id, target.clone())
                        })
                        .id(("target-language", n))
                        .justify_start()
                        .rounded(px(9.)),
                    );
                }
                if !found {
                    rows = rows.child(tr("Aucune langue trouvée"));
                }
                let popup = col()
                    .id(("languages", ix))
                    .occlude()
                    .w(px(272.))
                    .p(px(6.))
                    .gap(px(5.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(rgb(p.edge))
                    .bg(rgb(p.raised))
                    .shadow(p.bevel())
                    .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                        s.language_picker = None;
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.key == "escape" {
                            s.language_picker = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(input(
                        cx,
                        &self.language_search,
                        "filter-language",
                        "Rechercher une langue",
                    ))
                    .child(ScrollArea::new("language-results", rows).limited(208.))
                    .with_animation("language-picker", reveal(cx, 140), |d, t| {
                        d.relative().top(px(-3. * (1. - t))).opacity(t)
                    });
                anchor.child(
                    div().absolute().top_full().right_0().child(deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .snap_to_window_with_margin(px(10.))
                            .child(popup),
                    )),
                )
            })
    }

    fn reading_list(&self) -> &ListState {
        if self.tab == 0 {
            &self.report_list
        } else if self.tab == REVIEW_TAB {
            &self.review_list
        } else {
            &self.dialogues
        }
    }
    pub(super) fn reading_scrollbar(&self, cx: &Context<Self>) -> Stateful<Div> {
        scroller::rail(
            scroller::Axis::Rows(self.reading_list().clone()),
            if self.tab == 0 {
                "Parcourir le résumé"
            } else {
                "Parcourir les passages"
            },
            self.scroll_grab.clone(),
            *cx.global::<Palette>(),
        )
    }
}
