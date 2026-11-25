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
    pub(super) fn dialogue_list(&self, cx: &Context<Self>) -> Div {
        let state = self.dialogues.clone();
        div()
            .flex()
            .min_h_0()
            .flex_1()
            .child(
                list(
                    state,
                    cx.processor(|s, ix, _, cx| {
                        s.segment_card(ix, &s.segments[ix], cx).into_any_element()
                    }),
                )
                .flex_1()
                .min_w_0()
                .h_full(),
            )
            .child(self.reading_scrollbar(cx))
    }
    fn segment_card(&self, ix: usize, seg: &Segment, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let id = seg.id;
        let length = seg.end_ms - seg.start_ms;
        let current = self.playing.filter(|v| v.id == id);
        let playing = current.is_some_and(|v| !v.paused);
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
        let mut info = col()
            .flex_1()
            .min_w_0()
            .gap(px(4.))
            .child(
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
            )
            .child(
                div().flex().items_center().gap_2().child(seeker).child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(duration(length)),
                ),
            );
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
                                if let Some(seg) = s.segments.iter_mut().find(|v| v.id == id) {
                                    seg.text = text;
                                }
                                s.translations = s.db.translations(s.id)?;
                                s.edit_id = None;
                                s.dialogues.remeasure_items(ix..ix + 1);
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
                                s.dialogues.remeasure_items(ix..ix + 1);
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
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(seg) = s.segments.get(ix).filter(|v| v.id == id) {
                            let previous = s
                                .edit_id
                                .and_then(|old| s.segments.iter().position(|v| v.id == old));
                            s.edit.update(cx, |v, cx| v.emplace(&seg.text, cx));
                            s.edit_id = Some(id);
                            if let Some(i) = previous {
                                s.dialogues.remeasure_items(i..i + 1);
                            }
                            s.dialogues.remeasure_items(ix..ix + 1);
                            cx.notify();
                        }
                    })),
            );
        }
        if seg.echo_candidate.is_some() {
            info = info.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .child(tr("Reprise acoustique possible")),
            );
        }
        if let Some(items) = self.translations.get(&id) {
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
        let translate =
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
                        .child(
                            div()
                                .id("language-results")
                                .max_h(px(208.))
                                .overflow_y_scroll()
                                .child(rows),
                        )
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
                });
        let actions = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .flex_shrink_0()
            .child(
                button(
                    format!(
                        "{} le passage §{id}",
                        if playing {
                            "Mettre en pause"
                        } else {
                            "Écouter"
                        }
                    ),
                    Button::Secondary,
                    Some(if playing { PAUSE } else { PLAY }),
                    true,
                    cx,
                    move |s, _| s.listen(id, None),
                )
                .id(("passage-play", ix))
                .size(px(31.))
                .p_0(),
            )
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
            .child(translate);
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

    fn reading_list(&self) -> &ListState {
        if self.tab == 0 {
            &self.report_list
        } else {
            &self.dialogues
        }
    }
    pub(super) fn reading_scrollbar(&self, cx: &Context<Self>) -> Stateful<Div> {
        let list = self.reading_list().clone();
        let p = *cx.global::<Palette>();
        let release = |s: &mut Desk, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Desk>| {
            if s.scroll_grab.take().is_some() {
                s.reading_list().scrollbar_drag_ended();
                cx.notify();
            }
        };
        let drag = cx.listener(|s, e: &MouseMoveEvent, _, cx| {
            if let Some(grab) = s.scroll_grab {
                s.drag_scroll(
                    e.position.y - s.reading_list().viewport_bounds().origin.y,
                    grab,
                );
                cx.notify();
            }
        });
        div()
            .id("record-scrollbar")
            .role(Role::ScrollBar)
            .aria_label(tr(if self.tab == 0 {
                "Parcourir le résumé"
            } else {
                "Parcourir les passages"
            }))
            .focusable()
            .tab_stop(true)
            .relative()
            .w(px(12.))
            .h_full()
            .flex_shrink_0()
            .rounded_full()
            .opacity(if self.scroll_grab.is_some() {
                0.95
            } else {
                0.53
            })
            .hover(|d| d.opacity(0.85))
            .focus_visible(|d| d.opacity(1.0))
            .transitions(|t| t.opacity(motion(115)))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, w, _| {
                        if let Some((top, height, _)) = thumb(&list, bounds.size.height) {
                            let rect = Bounds::new(
                                point(bounds.center().x - px(2.5), bounds.origin.y + top),
                                size(px(5.), height),
                            );
                            w.paint_quad(
                                PaintQuad::new(rect)
                                    .background(rgb(p.muted))
                                    .corner_radii(px(2.5)),
                            );
                        }
                    },
                )
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &MouseDownEvent, _, cx| {
                    let b = s.reading_list().viewport_bounds();
                    if let Some((top, height, _)) = thumb(s.reading_list(), b.size.height) {
                        let y = e.position.y - b.origin.y;
                        let grab = if y >= top && y <= top + height {
                            y - top
                        } else {
                            height * 0.5
                        };
                        s.scroll_grab = Some(grab);
                        s.reading_list().scrollbar_drag_started();
                        s.drag_scroll(y, grab);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move_all(move |event, phase, _, w, cx| {
                if phase.bubble() {
                    drag(event, w, cx);
                }
            })
            .on_mouse_up(MouseButton::Left, cx.listener(release))
            .on_mouse_up_out(MouseButton::Left, cx.listener(release))
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                let page = s.reading_list().viewport_bounds().size.height;
                let y = s.reading_list().scroll_px_offset_for_scrollbar().y;
                let target = match e.keystroke.key.as_str() {
                    "home" => px(0.),
                    "end" => -s.reading_list().max_offset_for_scrollbar().y,
                    "pageup" => y + page,
                    "pagedown" => y - page,
                    "up" => y + px(32.),
                    "down" => y - px(32.),
                    _ => return,
                };
                s.reading_list()
                    .set_offset_from_scrollbar(point(px(0.), target));
                cx.stop_propagation();
                cx.notify();
            }))
    }
    fn drag_scroll(&self, y: Pixels, grab: Pixels) {
        let h = self.reading_list().viewport_bounds().size.height;
        if let Some((_, handle, max)) = thumb(self.reading_list(), h) {
            let fraction = ((y - grab) / (h - handle).max(px(1.))).clamp(0., 1.);
            self.reading_list()
                .set_offset_from_scrollbar(point(px(0.), -max * fraction));
        }
    }
}
fn thumb(list: &ListState, h: Pixels) -> Option<(Pixels, Pixels, Pixels)> {
    let max = list.max_offset_for_scrollbar().y;
    if max <= px(0.) || h <= px(0.) {
        return None;
    }
    let handle = (h * (h / (max + h))).max(px(24.)).min(h);
    let frac = (-list.scroll_px_offset_for_scrollbar().y / max).clamp(0., 1.);
    Some(((h - handle) * frac, handle, max))
}
