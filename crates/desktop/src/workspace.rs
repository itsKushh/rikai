use super::*;

#[derive(IntoElement)]
struct TabBackground {
    group: &'static str,
    target: Bounds<Pixels>,
    palette: Palette,
}
impl RenderOnce for TabBackground {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let target = self.target;
        let slide = w.use_keyed_transition(self.group, cx, motion(210), |_, _| target);
        if *slide.read_goal(cx) != target {
            slide.update(cx, |bounds, _| *bounds = target);
        }
        let b = if cx.reduce_motion() {
            target
        } else {
            *slide.evaluate(w, cx)
        };
        div()
            .absolute()
            .left(b.origin.x)
            .top(px(3.))
            .bottom(px(3.))
            .w(b.size.width)
            .rounded(px(11.))
            .bg(rgb(self.palette.selected))
            .shadow(self.palette.bevel())
    }
}

pub(super) fn strip(
    p: Palette,
    group: &'static str,
    index: usize,
    geometry: &std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
) -> Div {
    let radius = 15.;
    let indicator = geometry.get();
    let geometry = geometry.clone();
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(2.))
        .p(px(3.))
        .relative()
        .rounded(px(radius))
        .bg(rgb(p.base))
        .border_1()
        .border_color(linear_gradient(
            180.,
            linear_color_stop(black().opacity(0.68), 0.),
            linear_color_stop(rgb(p.muted).opacity(0.19), 1.),
        ))
        .shadow(p.pressed())
        .child(
            div()
                .absolute()
                .inset_0()
                .when(indicator.size.width > px(0.), |layer| {
                    layer.child(TabBackground {
                        group,
                        target: indicator,
                        palette: p,
                    })
                }),
        )
        .on_children_prepainted(move |bounds, w, _| {
            if let (Some(layer), Some(tab)) = (bounds.first(), bounds.get(index + 1)) {
                let target = Bounds::new(tab.origin - layer.origin, tab.size);
                if target != geometry.get() {
                    geometry.set(target);
                    w.request_animation_frame();
                }
            }
        })
}

fn header(title: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .min_h(px(40.))
        .px_4()
        .flex_shrink_0()
        .border_b_1()
        .border_color(rgb(p.edge))
        .child(label(title))
}

fn scroll(id: &'static str, content: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(ScrollArea::new(id, content))
}

fn fact(name: &'static str, value: impl Into<String>, p: Palette) -> Div {
    let value = value.into();
    let value = if matches!(
        name,
        "État"
            | "Profil conseillé"
            | "Calcul préféré"
            | "Intervenants"
            | "Compte rendu"
            | "Identification des intervenants"
            | "Résumé"
            | "Transcription"
    ) {
        crate::i18n::message(&value)
    } else {
        value
    };
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .py(px(5.))
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(p.muted))
                .child(tr(name)),
        )
        .child(div().min_w_0().text_right().child(value))
}

fn selected_file(settings: &Value, key: &str) -> String {
    settings[key]
        .as_str()
        .and_then(|p| std::path::Path::new(p).file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Aucun".into())
}

impl Desk {
    pub(super) fn drawer_open(&self) -> bool {
        self.confirmation.is_some() || self.page == Page::Settings
    }

    fn workspace_page(&self) -> Page {
        if matches!(
            self.page,
            Page::Settings | Page::ConfirmDownload(_) | Page::StorageChange
        ) {
            self.settings_origin
        } else {
            self.page
        }
    }

    fn dismiss_drawer(&mut self) {
        if self.confirmation.take().is_some() {
            return;
        }
        if !self.busy() {
            self.pending_translation = None;
            self.pending_summary = false;
            self.menu = None;
            self.page = self.settings_origin;
            self.message.clear();
        }
    }

    fn toolbar(&self, window: &Window, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let compact = window_frame::compact(window);
        let locked = self.busy();
        let page = self.workspace_page();
        let mut nav = strip(
            p,
            "navigation-pill",
            match page {
                Page::Modules | Page::Live | Page::Media | Page::Follow => 1,
                Page::Library | Page::Project | Page::Detail => 2,
                Page::Profiles => 3,
                _ => 0,
            },
            &self.tab_geometry[0],
        );
        for (target, name, icon) in [
            (Page::Home, "Espace de travail", DESK),
            (Page::Modules, "Modules", APPLICATIONS),
            (Page::Library, "Bibliothèque", LIBRARY),
            (Page::Profiles, "Voix", VOICES),
        ] {
            let active = page == target
                || (target == Page::Home && page == Page::New)
                || (target == Page::Library && matches!(page, Page::Detail | Page::Project))
                || (target == Page::Modules
                    && matches!(page, Page::Live | Page::Media | Page::Follow));
            if locked {
                nav = nav.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px_2()
                        .h(px(26.))
                        .rounded(px(8.))
                        .child(glyph(icon, 15., p.muted))
                        .when(!compact || active, |s| s.child(tr(name))),
                );
            } else {
                nav = nav.child(button(
                    name,
                    if active { Button::Tab } else { Button::IdleTab },
                    Some(icon),
                    compact && !active,
                    cx,
                    move |s, _| s.go(target),
                ));
            }
        }
        let mut bar = window_frame::header(p, nav, !self.drawer_open() && !self.release_was_open);
        if !locked {
            bar = bar
                .child(button(
                    "Paramètres",
                    Button::Secondary,
                    Some(SLIDERS),
                    true,
                    cx,
                    |s, _| {
                        let live = s.page == Page::Live;
                        s.go(Page::Settings)?;
                        if live {
                            s.section = 7;
                        } else if s.section >= 6 {
                            s.section = 0;
                        }
                        Ok(())
                    },
                ))
                .when(
                    page != Page::Live && !(page == Page::Home && self.library.is_empty()),
                    |bar| {
                        bar.child(button(
                            "Enregistrer",
                            Button::Primary,
                            Some(WAVE),
                            false,
                            cx,
                            |s, _| s.go(Page::New),
                        ))
                    },
                );
        }
        bar.child(window_frame::controls(
            window,
            p,
            !self.drawer_open(),
            cx.listener(|s, _, w, cx| {
                if s.can_close(cx) {
                    w.remove_window();
                }
            }),
        ))
    }

    fn breadcrumb(&self, page: Page, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut path = vec![(Page::Home, tr("Espace de travail").to_owned())];
        match page {
            Page::New => path.push((page, tr("Enregistrer").into())),
            Page::Modules | Page::Media | Page::Follow | Page::Live => {
                path.push((Page::Modules, tr("Modules").into()));
                if page != Page::Modules {
                    path.push((
                        page,
                        tr(match page {
                            Page::Media => "Export audio et vidéo vers texte",
                            Page::Follow => "Suivi entre échanges",
                            _ => "Traduction en direct",
                        })
                        .into(),
                    ));
                }
            }
            Page::Project => {
                path.push((Page::Library, tr("Bibliothèque").into()));
                path.push((page, self.folder_title()));
            }
            Page::Profiles => path.push((page, tr("Voix").into())),
            Page::Library | Page::Detail => {
                path.push((Page::Library, tr("Bibliothèque").into()));
                if page == Page::Detail {
                    if self.project_folder.is_some() {
                        path.push((Page::Project, self.folder_title()));
                    }
                    let title = self
                        .library
                        .iter()
                        .find(|m| m.id == self.id)
                        .map(|m| m.title.clone())
                        .unwrap_or_else(|| tr("Enregistrement").into());
                    path.push((Page::Detail, title));
                    path.push((Page::Detail, tr(RECORD_TABS[self.tab as usize]).into()));
                }
            }
            _ => {}
        }
        let mut trail = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(6.))
            .px(px(17.))
            .h(px(22.))
            .flex_shrink_0()
            .text_size(px(11.));
        let last = path.len() - 1;
        for (i, (target, title)) in path.into_iter().enumerate() {
            if i > 0 {
                trail = trail.child(div().text_color(rgb(p.muted)).child("›"));
            }
            if i == last {
                trail = trail.child(div().truncate().child(title));
            } else {
                trail = trail.child(
                    div()
                        .id(("crumb", i))
                        .role(Role::Button)
                        .focusable()
                        .tab_stop(true)
                        .truncate()
                        .cursor_pointer()
                        .text_color(rgb(p.muted))
                        .hover(|d| d.text_color(rgb(p.text)))
                        .focus_visible(|d| d.text_color(rgb(p.text)))
                        .transitions(|t| t.text_color(motion(120)))
                        .child(title)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if !s.busy() {
                                let result = if target == Page::Detail {
                                    s.tab = 0;
                                    s.detail(s.id)
                                } else {
                                    s.go(target)
                                };
                                if let Err(e) = result {
                                    s.message = e.to_string();
                                }
                                cx.notify();
                            }
                        })),
                );
            }
        }
        trail
    }

    pub(super) fn preview(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut items = col().gap(px(3.));
        for entry in &self.excerpts {
            let id = entry.recording;
            let segment = entry.segment;
            let line = div()
                .id(("recording-excerpt", id as u64))
                .role(Role::Button)
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .rounded(px(9.))
                .px_3()
                .py_2()
                .bg(rgb_to_hsla(rgb(p.control)).opacity(0.))
                .hover(|d| d.bg(rgb(p.control)))
                .transitions(|t| t.bg(motion(120)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(raw_label(entry.title.clone()).truncate())
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(p.muted))
                                .child(time(entry.duration_ms as u64)),
                        ),
                )
                .when(segment.is_some(), |d| {
                    d.child(
                        div()
                            .flex()
                            .gap_2()
                            .text_color(rgb(p.muted))
                            .child(div().flex_shrink_0().child(entry.speaker.clone()))
                            .child(div().truncate().child(entry.text.clone())),
                    )
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Err(e) = s.detail(id) {
                        s.message = e.to_string();
                    } else if let Some(i) = s.segments.iter().position(|x| Some(x.id) == segment) {
                        s.tab = 1;
                        s.dialogues.scroll_to_reveal_item(i);
                    }
                    cx.notify();
                }));
            items = items.child(line);
        }
        if self.excerpts.is_empty() {
            items = items.child(div().p_2().text_color(rgb(p.muted)).child(tr(
                if self.browse_term.is_empty() {
                    "Vos extraits apparaîtront ici après la capture."
                } else {
                    "Aucun enregistrement trouvé."
                },
            )));
        }
        col()
            .size_full()
            .p_2()
            .child(label("Parcourir les enregistrements"))
            .child(input(
                cx,
                &self.browse_query,
                "browse-recordings",
                "Rechercher un titre, un intervenant ou une phrase",
            ))
            .child(scroll("home-excerpts", items))
    }

    fn inspector_panel(&self, page: Page, passive: bool, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut tabs = strip(
            p,
            "inspector-pill",
            self.inspector as usize,
            &self.tab_geometry[1],
        )
        .w_full();
        for (tab, title, icon) in [
            (Inspector::Session, "Session", SESSION),
            (Inspector::Audio, "Audio", AUDIO),
            (Inspector::Models, "Modèles", MODEL),
        ] {
            let style = if self.inspector == tab {
                Button::Tab
            } else {
                Button::IdleTab
            };
            tabs = if passive {
                tabs.child(div().flex_1().text_center().py_1().child(tr(title)))
            } else {
                tabs.child(
                    button(title, style, Some(icon), false, cx, move |s, _| {
                        s.inspector = tab;
                        Ok(())
                    })
                    .flex_1()
                    .rounded(px(11.))
                    .px(px(4.)),
                )
            };
        }
        let mut content = col().p_4();
        match self.inspector {
            Inspector::Session => {
                let meeting = match page {
                    Page::Detail => self.library.iter().find(|m| m.id == self.id),
                    Page::Home => self.library.first(),
                    _ => None,
                };
                content = content.child(label("Informations de session"));
                if let Some(m) = meeting {
                    content = content
                        .child(fact("Enregistrement", m.title.clone(), p))
                        .child(fact("Date", m.date.clone(), p))
                        .child(fact("Durée", time(m.duration_ms as u64), p))
                        .child(fact("État", m.status.clone(), p));
                    if !m.project.is_empty() {
                        content = content.child(fact("Projet", m.project.clone(), p));
                    }
                } else {
                    content = content
                        .child(fact(
                            "État",
                            if self.ctl.is_some() {
                                "Enregistrement"
                            } else if self.cancel.is_some() {
                                "Traitement"
                            } else {
                                "En attente"
                            },
                            p,
                        ))
                        .child(fact(
                            "Sources sélectionnées",
                            self.chosen.len().to_string(),
                            p,
                        ));
                }
                content = content
                    .child(label("Enregistrement").mt_3())
                    .child(fact(
                        "Intervenants",
                        if self.settings["diarization"] == true {
                            "Identification activée"
                        } else {
                            "Identification désactivée"
                        },
                        p,
                    ))
                    .child(fact(
                        "Compte rendu",
                        if self.settings["auto_summary"] == true {
                            "Automatique"
                        } else {
                            "À la demande"
                        },
                        p,
                    ));
            }
            Inspector::Audio => {
                content = content.child(label("Sources audio"));
                if self.ctl.is_some() {
                    content = content
                        .children(self.capture_names.iter().cloned().map(|n| div().child(n)));
                } else if !self.chosen.is_empty() {
                    content = content.children(
                        self.inputs
                            .iter()
                            .filter(|i| self.chosen.contains(&i.source))
                            .map(|i| div().child(i.name.clone())),
                    );
                } else {
                    content = content.child(
                        div()
                            .text_color(rgb(p.muted))
                            .child(tr("Aucune source sélectionnée.")),
                    );
                }
                content = content
                    .child(label("Format des pistes").mt_3())
                    .child(fact("Canaux", "Mono", p))
                    .child(fact("Fréquence", "16 kHz", p))
                    .child(fact("Encodage", "PCM 16 bits", p));
            }
            Inspector::Models => {
                content = content
                    .child(label("Modèles locaux"))
                    .child(fact(
                        "Transcription",
                        if self.settings["whisper"].is_string() {
                            "Configurée"
                        } else {
                            "À configurer"
                        },
                        p,
                    ))
                    .child(fact("Résumé", selected_file(&self.settings, "gguf"), p))
                    .child(fact(
                        "Calcul préféré",
                        if self.settings["device"] == "cuda" {
                            "CUDA"
                        } else {
                            "CPU"
                        },
                        p,
                    ))
                    .child(fact("Profil conseillé", self.recommended.clone(), p));
                if !passive && !self.busy() {
                    content =
                        content.child(action("Configurer les modèles", false, cx, |s, _| {
                            s.go(Page::Settings)?;
                            s.section = 0;
                            Ok(())
                        }));
                }
            }
        }
        p.panel()
            .child(
                div()
                    .p(px(5.))
                    .border_b_1()
                    .border_color(rgb(p.edge))
                    .child(tabs),
            )
            .child(scroll("inspector-body", content))
    }

    fn settings_drawer(&self, visibility: f32, w: &Window, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let (title, fields) = if let Some(prompt) = &self.confirmation {
            (
                prompt.title.to_owned(),
                col().child(crate::i18n::message(&prompt.detail)).child(
                    row()
                        .child(action("Annuler", false, cx, |s, _| {
                            s.confirmation = None;
                            Ok(())
                        }))
                        .child(action(prompt.accept, true, cx, |s, _| {
                            if let Some(prompt) = s.confirmation.take() {
                                (prompt.action)(s)?;
                            }
                            Ok(())
                        })),
                ),
            )
        } else {
            self.page_body(
                if self.drawer_open() {
                    self.page
                } else {
                    Page::Settings
                },
                true,
                cx,
            )
        };
        let close = button(
            "Fermer le panneau",
            Button::Quiet,
            Some(CLOSE),
            true,
            cx,
            |s, _| {
                s.dismiss_drawer();
                Ok(())
            },
        );
        let close = close
            .size(px(28.))
            .min_w(px(28.))
            .min_h(px(28.))
            .p_0()
            .aspect_ratio(1.)
            .flex_shrink_0();
        let body = col()
            .gap(px(16.))
            .when(!self.message.is_empty(), |s| {
                s.child(notice(&self.message, p))
            })
            .child(fields);
        let side = (self.page == Page::Settings && self.confirmation.is_none() && !self.busy())
            .then(|| self.settings_sidebar(w, cx));
        let sheet = preferences_panel::sheet(
            title,
            (!self.busy() || self.confirmation.is_some()).then_some(close),
            side,
            body,
            self.settings_scroll.clone(),
            p,
        )
        .when(self.confirmation.is_some(), |s| {
            s.max_w(px(440.)).max_h(px(220.))
        })
        .track_focus(&self.drawer_focus)
        .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
            if e.keystroke.key == "escape" {
                if s.menu.take().is_none() {
                    s.dismiss_drawer();
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
                    s.dismiss_drawer();
                    cx.notify();
                })),
        )
    }
}

fn notice(message: &str, p: Palette) -> Div {
    div()
        .px_3()
        .py_2()
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(p.edge))
        .bg(rgb(p.control))
        .child(crate::i18n::message(message))
}

impl Render for Desk {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let narrow = window.viewport_size().width < px(810.);
        let package = matches!(self.page, Page::ConfirmDownload(_) | Page::StorageChange);
        let update = !package
            && self.confirmation.is_none()
            && self.update_info.is_some()
            && (self.cancel.is_none() && self.ctl.is_none() && self.preview.is_none()
                || self.update_downloading);
        let modal = update || package;
        if modal && !self.release_was_open {
            self.release_focus.focus(window, cx);
        }
        if !modal && self.release_was_open && self.drawer_open() {
            self.drawer_focus.focus(window, cx);
        }
        self.release_was_open = modal;
        let drawer = self.drawer_open();
        if self.project_form.is_some() && !self.drawer_focus.contains_focused(window, cx) {
            self.drawer_focus.focus(window, cx);
        }
        let visibility = preferences_panel::visibility(drawer, window, cx);
        if drawer && !self.drawer_was_open && !update {
            self.drawer_focus.focus(window, cx);
        }
        self.drawer_was_open = drawer;
        let page = self.workspace_page();
        let background_activity = !modal && (!drawer || self.confirmation.is_some());
        let reader = page == Page::Detail
            && matches!(self.tab, 0 | 1 | REVIEW_TAB | 4)
            && !self.metadata
            && !self.busy();
        let home = page == Page::Home && !(self.busy() && background_activity);
        let mut left = col().min_h_0().flex_1().gap(px(16.));
        if home {
            left = left
                .child(
                    p.panel()
                        .flex_1()
                        .min_h(px(160.))
                        .p(px(8.))
                        .child(self.preview(cx)),
                )
                .child(
                    p.panel()
                        .h(if narrow {
                            px(180.)
                        } else {
                            window.viewport_size().height * 0.31
                        })
                        .child(header("Enregistrements récents", p))
                        .child(scroll(
                            "recent-recordings",
                            self.meetings_view(cx, 10, !drawer),
                        )),
                );
        } else {
            let (title, body) = self.page_body(page, background_activity, cx);
            let detail = page == Page::Detail && !(self.busy() && background_activity);
            let content = body.p_4().when(detail, |d| d.pt(px(9.)));
            left = left.child(
                p.panel()
                    .flex_1()
                    .child(if detail {
                        self.record_header(title, cx)
                    } else {
                        if matches!(page, Page::Library | Page::Project) {
                            row()
                                .justify_between()
                                .px_3()
                                .py_2()
                                .child(raw_label(tr(&title).to_owned()))
                                .child(self.library_header_action(cx))
                        } else {
                            header(title, p)
                        }
                    })
                    .when(detail, |d| {
                        d.child(
                            div()
                                .px_4()
                                .pt_4()
                                .flex_shrink_0()
                                .child(self.detail_tabs(cx)),
                        )
                    })
                    .child(if reader {
                        content.into_any_element()
                    } else {
                        scroll("workspace-content", content).into_any_element()
                    }),
            );
        }
        let inspector = self.inspector_panel(page, drawer, cx);
        let standalone = matches!(
            page,
            Page::Modules | Page::Media | Page::Follow | Page::Project | Page::Library | Page::Live
        ) || page == Page::Home && self.library.is_empty() && home;
        let layout = if page == Page::Live
            && !(drawer && self.section >= 6)
            && let Some(l) = self.live_view.clone()
        {
            col().relative().size_full().child(l).into_any_element()
        } else if page == Page::Modules && !self.busy() {
            col()
                .size_full()
                .items_center()
                .child(
                    col()
                        .w_full()
                        .max_w(px(1010.))
                        .px_4()
                        .pt(px(46.))
                        .gap_4()
                        .child(div().text_2xl().child(tr("Modules")))
                        .child(self.module_cards(cx)),
                )
                .into_any_element()
        } else if home && self.library.is_empty() {
            self.first_home(cx).into_any_element()
        } else if standalone {
            left.size_full().into_any_element()
        } else if narrow {
            ScrollArea::new(
                "compact-workspace",
                col()
                    .gap_3()
                    .child(
                        left.min_h(px(320.))
                            .when(page == Page::Detail || home, |d| {
                                d.h((window.viewport_size().height - px(105.)).max(px(320.)))
                                    .flex_shrink_0()
                            }),
                    )
                    .child(inspector.min_h(px(225.))),
            )
            .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .gap(px(16.))
                .child(left)
                .child(inspector.w(relative(0.323)).min_w(px(272.)).flex_shrink_0())
                .into_any_element()
        };
        window_frame::surface(p)
            .capture_key_down(cx.listener(|s, ev: &KeyDownEvent, w, cx| {
                if s.project_form.is_some() && ev.keystroke.key == "escape" {
                    s.project_form = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if s.handle_shortcut(ev, w, cx) {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if ev.keystroke.key == "escape"
                    && (s.source_menu.take().is_some() || s.record_menu.take().is_some())
                {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if ev.keystroke.key != "tab"
                    || ev.keystroke.modifiers.control
                    || ev.keystroke.modifiers.alt
                {
                    return;
                }
                let step = if ev.keystroke.modifiers.shift {
                    Window::focus_prev
                } else {
                    Window::focus_next
                };
                step(w, cx);
                if s.release_was_open || s.drawer_open() || s.project_form.is_some() {
                    let modal = if s.release_was_open {
                        &s.release_focus
                    } else {
                        &s.drawer_focus
                    };
                    let first = w.focused(cx);
                    while !modal.contains_focused(w, cx) {
                        step(w, cx);
                        if w.focused(cx) == first {
                            modal.focus(w, cx);
                            break;
                        }
                    }
                }
                cx.stop_propagation();
            }))
            .child(self.toolbar(window, cx))
            .child(self.breadcrumb(page, cx))
            .child(
                div()
                    .relative()
                    .min_h_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px(px(13.))
                    .pb(px(12.))
                    .pt(px(8.))
                    .when(!drawer && !self.message.is_empty(), |s| {
                        s.child(notice(&self.message, p).mb_2())
                    })
                    .child(div().flex_1().min_h_0().child(layout)),
            )
            .when(!package && (drawer || visibility > 0.001), |s| {
                s.child(self.settings_drawer(visibility, window, cx))
            })
            .when(!modal && !drawer && self.project_form.is_some(), |s| {
                s.child(self.project_modal(cx))
            })
            .when(
                !drawer
                    && !modal
                    && self.project_form.is_none()
                    && !self.busy()
                    && !self.live_active,
                |s| {
                    s.on_drop(cx.listener(|desk, paths: &ExternalPaths, _, cx| {
                        if let [path] = paths.paths() {
                            desk.choose_media(path.clone(), desk.page == Page::Media, cx)
                        } else {
                            desk.message = "Déposez un seul fichier audio ou vidéo.".into();
                        }
                        cx.notify();
                    }))
                    .when(
                        cx.active_drag()
                            .is_some_and(|d| d.value.is::<ExternalPaths>()),
                        |s| {
                            s.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(black().opacity(0.55))
                                    .backdrop_blur(px(7.))
                                    .child(label("Déposez un fichier audio ou vidéo"))
                                    .with_animation("drop-reveal", reveal(cx, 160), |d, t| {
                                        d.opacity(t)
                                    }),
                            )
                        },
                    )
                },
            )
            .when(package, |s| s.child(self.package_modal(cx)))
            .when(update, |s| s.child(self.release_modal(cx)))
    }
}
