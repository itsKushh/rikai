use super::appearance::*;
use super::*;
use gpui::Div;
#[path = "budgets.rs"]
pub(super) mod budgets;
#[path = "dialogue.rs"]
mod dialogue;
#[path = "record.rs"]
mod record;

#[derive(Clone, Copy)]
enum Button {
    Primary,
    Secondary,
    Quiet,
    Selected,
    Tab,
    IdleTab,
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
    control(label, style, icon, icon_only, cx).on_click(cx.listener(move |s, _, w, cx| {
        s.message.clear();
        if let Err(e) = f(s, w, cx) {
            s.message = format!("{e:#}");
        }
        cx.notify();
    }))
}

fn control(
    label: impl Into<String>,
    style: Button,
    icon: Option<&'static str>,
    icon_only: bool,
    cx: &App,
) -> Stateful<Div> {
    let label = label.into();
    let p = *cx.global::<Palette>();
    let (bg, fg) = match style {
        Button::Primary => (p.primary, p.on_primary),
        Button::Secondary => (p.control, p.text),
        Button::Selected => (p.selected, p.text),
        Button::Tab | Button::IdleTab => (p.selected, p.text),
        Button::Quiet => (p.base, p.muted),
    };
    let tab = matches!(style, Button::Tab | Button::IdleTab);
    let quiet = matches!(style, Button::Quiet) || tab;
    let display = crate::i18n::message(&label);
    let hint = display.clone();
    let hover_bg = match style {
        Button::Primary => {
            if p.base == 0x111113 {
                0xcacace
            } else {
                0x414147
            }
        }
        Button::Secondary | Button::Selected => {
            if p.base == 0x111113 {
                0x36363a
            } else {
                0xe0e0e4
            }
        }
        Button::Quiet => p.control,
        Button::Tab | Button::IdleTab => p.selected,
    };
    div()
        .id(SharedString::from(label.clone()))
        .aria_label(display.clone())
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .min_h(px(28.))
        .px(px(9.))
        .py(px(3.))
        .rounded(px(11.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .role(gpui::Role::Button)
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .relative()
        .top(px(0.))
        .bg(if quiet {
            rgb_to_hsla(rgb(hover_bg)).opacity(0.)
        } else {
            rgb_to_hsla(rgb(bg))
        })
        .border_1()
        .border_color(if quiet {
            transparent_black()
        } else {
            rgb_to_hsla(rgb(p.edge)).opacity(0.25)
        })
        .when(tab, |s| s.rounded(px(11.)))
        .when(!quiet, |s| s.shadow(p.bevel()))
        .text_color(rgb(fg))
        .when(!tab, |d| d.hover(move |s| s.bg(rgb(hover_bg))))
        .when(tab, |d| {
            d.opacity(if matches!(style, Button::IdleTab) {
                0.66
            } else {
                1.
            })
            .hover(|s| s.opacity(1.))
            .active(|s| s.opacity(1.))
        })
        .focus_visible(move |s| s.border_color(rgb(p.text)))
        .when(!tab, |d| {
            d.active(move |s| s.top(px(0.60)).shadow(p.pressed()))
        })
        .transitions(|t| {
            t.bg(motion(120))
                .opacity(motion(120))
                .top(motion(90))
                .border_color(motion(100))
        })
        .when(icon_only, |b| {
            b.tooltip(move |_, cx| cx.new(|_| ControlHint(hint.clone())).into())
        })
        .when_some(icon, |s, icon| s.child(glyph(icon, 15., fg)))
        .when(!icon_only, |s| s.child(display))
}
struct ControlHint(String);
impl Render for ControlHint {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        div()
            .font_family("Inter")
            .bg(rgb(p.raised))
            .text_color(rgb(p.text))
            .rounded(px(11.))
            .border_1()
            .border_color(rgb(p.edge))
            .shadow(p.bevel())
            .child(self.0.clone())
            .with_animation("hint-reveal", reveal(cx, 110), |el, t| {
                el.relative()
                    .top(px(3. * (1. - t)))
                    .px(px(11. + t))
                    .py(px(6. + t))
                    .text_size(px(11.6 + 0.4 * t))
                    .opacity(t)
            })
    }
}
// Courbe de Bézier cubique, points de contrôle (1/3, 0.78) et (2/3, 1).
// L’abscisse est linéaire, ce qui évite de résoudre la courbe à chaque image.
fn glide(t: f32) -> f32 {
    let u = 1. - t;
    3. * u * u * t * 0.78 + 3. * u * t * t + t * t * t
}
fn motion(ms: u64) -> Motion {
    millis(ms).with_easing(glide)
}

fn reveal(cx: &App, ms: u64) -> Animation {
    Animation::new(millis(if cx.reduce_motion() { 1 } else { ms })).with_easing(glide)
}
fn col() -> Div {
    div().min_w_0().flex().flex_col().gap(px(9.))
}
fn row() -> Div {
    div().flex().flex_wrap().items_center().gap_2()
}
fn bar(fraction: f32, color: u32, p: Palette) -> Div {
    div()
        .w_full()
        .h(px(5.))
        .flex_shrink_0()
        .overflow_hidden()
        .rounded_full()
        .bg(rgb(p.control))
        .child(
            div()
                .id("bar-fill")
                .h_full()
                .w(relative(fraction.clamp(0., 1.)))
                .transitions(|t| t.w(millis(90)))
                .rounded_full()
                .bg(rgb(color)),
        )
}

fn meter(peak: f32, p: Palette) -> Div {
    let db = 20. * peak.log10();
    let value = if db < -60. {
        "Silence".to_owned()
    } else if peak >= 0.9999 {
        "Saturation · 0 dBFS".to_owned()
    } else {
        format!("{db:.1} dBFS")
    };
    col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(p.muted))
                .child(crate::i18n::message(&value)),
        )
        .child(bar(
            (db + 60.) / 60.,
            if peak >= 0.9999 { p.text } else { p.level },
            p,
        ))
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
        .child(div().w(px(65.)).child(tr(duration).to_owned()))
        .child(div().w(px(110.)).truncate().child(tr(status).to_owned()))
}
fn label(s: impl Into<SharedString>) -> Div {
    raw_label(crate::i18n::message(s.into().as_ref()))
}
fn raw_label(s: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(s.into())
}
fn input(cx: &App, f: &Field, id: &'static str, hint: &'static str) -> impl IntoElement {
    let p = *cx.global::<Palette>();
    text_input(id)
        .state(f.downgrade())
        .placeholder(tr(hint))
        .focus(move |s| s.border_color(rgb(p.muted)))
        .transitions(|t| t.border_color(motion(100)))
        .caret_blink_interval_500ms()
        .px(px(9.))
        .py(px(6.))
        .min_h(px(29.))
        .rounded(px(11.))
        .shadow(p.bevel())
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
        let mut list = div().flex().flex_col().child(
            table_row(div().child(tr("Enregistrement")), "Durée", "État", p)
                .text_color(rgb(p.muted)),
        );
        if self.library.is_empty() {
            return list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child(tr("Aucun enregistrement trouvé.")),
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
                        .aria_label(crate::i18n::message(&format!("Ouvrir {}", m.title)))
                        .cursor_pointer()
                        .bg(rgb_to_hsla(rgb(p.control)).opacity(0.))
                        .hover(move |s| s.bg(rgb(p.control)))
                        .focus_visible(move |s| s.bg(rgb(p.selected)))
                        .transitions(|t| t.bg(motion(120)))
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
                    "Titre de l’enregistrement",
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
                let testing = self.preview.is_some();
                let control = checkbox(name, selected, testing, p)
                    .id(("source", index))
                    .justify_between()
                    .px_2()
                    .min_h(px(36.))
                    .child(div().flex_1())
                    .when(selected, |d| {
                        d.child(div().text_color(rgb(p.muted)).child(crate::i18n::message(
                            &format!("Gain {:+} dB", self.gain_db(&src)),
                        )))
                    })
                    .when(!testing && !selected, |d| {
                        d.child(
                            div()
                                .text_color(rgb(p.muted))
                                .child(crate::i18n::message(&format!("{:.0} %", i.peak * 100.))),
                        )
                    })
                    .aria_disabled(testing)
                    .tab_stop(!testing)
                    .when(!testing, |d| {
                        d.on_click(cx.listener(move |s, _, _, cx| {
                            if s.inputs.get(index).is_some_and(|i| i.source == src)
                                && !s.chosen.remove(&src)
                            {
                                if mic {
                                    s.chosen.retain(|v| !matches!(v, Source::Microphone { .. }));
                                }
                                s.chosen.insert(src.clone());
                            }
                            cx.notify();
                        }))
                    });
                body = body.child(control);
            }
            if !found {
                body = body.child(tr(if self.scanning {
                    "Détection en cours…"
                } else if mic {
                    "Aucune entrée disponible."
                } else {
                    "Aucune session détectée. Lancez un son dans l’application."
                }));
            }
        }
        if !self.chosen.is_empty() {
            body = body.child(self.level_setup(cx));
        }
        body.child(toggle("Identifier les intervenants","diarization",self,cx))
            .when(self.settings["diarization"]==true,|d|d.child(toggle("Proposer les voix connues","recognize",self,cx)))
            .child(toggle("Générer le compte rendu à l’arrêt","auto_summary",self,cx))
            .child(div().text_color(rgb(cx.global::<Palette>().muted)).child(tr("Informez les participants avant d’enregistrer. Un navigateur sélectionné inclut ses onglets.")))
            .child(action("Démarrer l’enregistrement",true,cx,|s,cx|s.start(cx)))
    }
    fn level_setup(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut head = row().justify_between().child(label("Niveaux des sources"));
        head = match &self.preview {
            Some(test) if test.stopping => head.child(tr("Fermeture des sources…")),
            Some(_) => head.child(action("Terminer le test", false, cx, |s, _| {
                s.stop_test(false);
                Ok(())
            })),
            None => head.child(action("Tester les niveaux", false, cx, |s, _| {
                s.test_levels()
            })),
        };
        let mut body = col()
            .border_t_1()
            .border_color(rgb(p.edge))
            .pt_3()
            .child(head);
        let Some(test) = &self.preview else {
            return body;
        };
        body = body.child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(
            if test.ctl.ready() < test.tracks.len() && !test.stopping {
                "Ouverture des sources…"
            } else {
                "Parlez ou lancez un son. Visez des crêtes entre −18 et −6 dBFS."
            },
        ));
        let levels = test.ctl.levels();
        for (index, source) in test.tracks.iter().enumerate() {
            let Some(input) = self.inputs.iter().find(|i| &i.source == source) else {
                continue;
            };
            let gain = self.gain_db(source);
            let name = &input.name;
            let mut controls = row().gap(px(6.));
            for (step, text, id) in [(-3, "−", "gain-down"), (3, "+", "gain-up")] {
                let source = source.clone();
                let bounded = !(audio_capture::MIN_GAIN_DB..=audio_capture::MAX_GAIN_DB)
                    .contains(&(gain + step));
                let btn = if bounded {
                    disabled_button(text, p)
                } else {
                    action(text, false, cx, move |s, _| {
                        s.set_gain(&source, s.gain_db(&source) + step)
                    })
                }
                .id((id, index))
                .aria_label(crate::i18n::message(&format!("{name} · gain {step:+} dB")));
                if step > 0 {
                    controls = controls.child(
                        div()
                            .min_w(px(41.))
                            .text_center()
                            .child(crate::i18n::message(&format!("{gain:+} dB"))),
                    );
                }
                controls = controls.child(btn);
            }
            let peak = levels
                .get(&format!("piste-{}.wav", index + 1))
                .copied()
                .unwrap_or(0.);
            body = body.child(
                col()
                    .gap(px(4.))
                    .child(
                        row()
                            .justify_between()
                            .child(raw_label(name.clone()))
                            .child(controls),
                    )
                    .child(meter(peak, p)),
            );
        }
        for notice in test.ctl.incidents().into_values() {
            body = body.child(crate::i18n::message(&notice));
        }
        body
    }

    fn detail_tabs(&self, cx: &Context<Self>) -> Div {
        workspace::strip(
            *cx.global::<Palette>(),
            "detail-pill",
            usize::from(self.tab),
            &self.tab_geometry[2],
        )
        .children(
            ["Résumé", "Transcription", "Intervenants"]
                .into_iter()
                .enumerate()
                .map(|(i, t)| {
                    button(
                        t,
                        if self.tab == i as u8 {
                            Button::Tab
                        } else {
                            Button::IdleTab
                        },
                        None,
                        false,
                        cx,
                        move |s, _| {
                            s.stop_segment();
                            s.record_menu = None;
                            s.metadata = false;
                            s.language_picker = None;
                            s.scroll_grab = None;
                            s.tab = i as u8;
                            s.edit_id = None;
                            Ok(())
                        },
                    )
                    .flex_1()
                }),
        )
    }

    fn detail_view(&self, cx: &Context<Self>) -> Div {
        let mut body = col();
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
                .child(
                    row()
                        .child(button("Annuler", Button::Quiet, None, false, cx, |s, _| {
                            s.metadata = false;
                            Ok(())
                        }))
                        .child(action("Enregistrer les informations", true, cx, |s, cx| {
                            s.db.update_metadata(
                                s.id,
                                s.title.read(cx).as_str(),
                                s.project.read(cx).as_str(),
                                s.description.read(cx).as_str(),
                                s.tags.read(cx).as_str(),
                            )?;
                            s.detail(s.id)
                        })),
                );
        }
        match self.tab {
            0 => return self.summary_reader(cx),
            1 => return self.transcript_reader(cx),
            _ => {
                let mut people = std::collections::BTreeMap::new();
                for seg in &self.segments {
                    *people
                        .entry((&seg.acoustic_label, &seg.speaker))
                        .or_insert(0) += 1;
                }
                if people.is_empty() {
                    body = body.child(tr("Aucune intervention transcrite."));
                }
                if people.keys().any(|(raw, _)| {
                    raw.starts_with("Intervenant ") && raw.as_str() != "Intervenant incertain"
                }) {
                    body = body.child(input(
                        cx,
                        &self.profile_name,
                        "profile-name",
                        "Nom de l’intervenant ou du profil autorisé",
                    ));
                }
                for ((raw, name), count) in people {
                    let mut person = row().gap(px(5.)).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(label(format!("{name} · {count} passages"))),
                    );
                    if raw.starts_with("Intervenant ") && raw != "Intervenant incertain" {
                        let acoustic = raw.to_owned();
                        person = person.child(
                            button(
                                format!("Renommer {raw} pour cet enregistrement"),
                                Button::Quiet,
                                Some(EDIT),
                                true,
                                cx,
                                move |s, cx| {
                                    s.db.name_speaker(
                                        s.id,
                                        &acoustic,
                                        s.profile_name.read(cx).as_str(),
                                    )?;
                                    s.detail(s.id)
                                },
                            )
                            .size(px(29.))
                            .p_0(),
                        );
                        if let Some(best) = self
                            .segments
                            .iter()
                            .filter(|p| p.acoustic_label == *raw && p.end_ms - p.start_ms >= 1500)
                            .max_by_key(|p| p.end_ms - p.start_ms)
                        {
                            let seg = best.clone();
                            person = person.child(
                                button(
                                    format!(
                                        "Créer un profil avec §{} · {}",
                                        seg.id,
                                        time(seg.start_ms as u64)
                                    ),
                                    Button::Quiet,
                                    Some(VOICES),
                                    true,
                                    cx,
                                    move |s, cx| {
                                        s.enroll(
                                            seg.clone(),
                                            s.profile_name.read(cx).as_str().to_owned(),
                                        )
                                    },
                                )
                                .size(px(29.))
                                .p_0(),
                            );
                        }
                    }
                    body = body.child(person);
                }
                for (raw, p, name, score) in &self.suggestions {
                    let mut suggestion = row().gap(px(5.)).child(div().flex_1().min_w_0().child(
                        crate::i18n::message(&format!(
                            "{raw} · proposition {name} · similarité {score:.2}"
                        )),
                    ));
                    for (txt, icon, choice) in
                        [("Confirmer", CHECK, Some(*p)), ("Refuser", CLOSE, None)]
                    {
                        let raw = raw.clone();
                        suggestion = suggestion.child(
                            button(
                                format!("{txt} · {raw}"),
                                Button::Quiet,
                                Some(icon),
                                true,
                                cx,
                                move |s, _| {
                                    s.db.confirm(s.id, &raw, choice)?;
                                    s.detail(s.id)
                                },
                            )
                            .size(px(29.))
                            .p_0(),
                        );
                    }
                    body = body.child(suggestion);
                }
            }
        }
        body
    }
    fn preference(&self, pref: Preference, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let (name, key, choices, current) = match pref {
            Preference::Locale => unreachable!(),
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
        .aria_label(crate::i18n::message(&format!(
            "{name} · {}",
            choices[current]
        )))
        .justify_between()
        .child(glyph(CHEVRON, 14., p.muted));
        let mut anchor = div().relative().child(field.w_full());
        if open {
            let mut menu = col()
                .gap(px(2.))
                .p(px(3.))
                .bg(rgb(p.raised))
                .rounded(px(12.))
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
                            Preference::Locale => unreachable!(),
                            Preference::Theme => json!(index == 1),
                            Preference::Device => json!(if index == 1 { "cuda" } else { "cpu" }),
                            Preference::Language => json!(if index == 1 { "" } else { "fr" }),
                        };
                        if pref == Preference::Device && index == 1 {
                            s.settings["strict_cuda"] = json!(true);
                        }
                        if pref == Preference::Theme {
                            cx.set_global(Palette::new(index == 1));
                            cx.refresh_windows();
                        }
                        s.save_settings()
                    },
                )
                .id((name, index))
                .rounded(px(9.))
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
                    .child(deferred(popover.with_animation(
                        "menu-reveal",
                        reveal(cx, 120),
                        |el, t| el.relative().top(px(-4. * (1. - t))).opacity(t),
                    ))),
            );
        }
        col().flex_1().gap(px(5.)).child(label(name)).child(anchor)
    }

    fn settings_view(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        match self.section {
            0 => self.models_settings(cx),
            1 => col()
                .gap(px(18.))
                .child(self.storage_settings(cx))
                .child(self.package_row("Traduction locale", "M2M100 · 100 langues · 496 Mo", cx))
                .child(
                    col()
                        .child(label("Moteur NVIDIA"))
                        .child(if self.nvidia_ready {
                            disabled_button("Déjà installé", p)
                        } else {
                            action("Installer le moteur NVIDIA", false, cx, |s, _| {
                                s.page = Page::ConfirmDownload("NVIDIA CUDA");
                                Ok(())
                            })
                        }),
                ),
            2 => col()
                .gap(px(18.))
                .child(self.locale_control(cx))
                .child(self.preference(Preference::Theme, cx)),
            3 => self.compute_settings(cx),
            _ => self.update_settings(cx),
        }
    }
    fn models_settings(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut presets = col().gap(px(2.)).child(label("Modèle de transcription"));
        for (preset, model) in
            Page::MODEL_PRESETS
                .into_iter()
                .zip(["base", "small", "medium", "large-v3-turbo"])
        {
            let recommended = preset == self.recommended;
            let repo = Self::repo_for(preset);
            let selected =
                self.settings["whisper_repo"].as_str() == Some(repo) && self.available(preset);
            presets = presets.child(
                (if selected {
                    disabled_button(preset, p)
                        .bg(transparent_black())
                        .opacity(0.85)
                        .shadow(vec![])
                } else {
                    button(preset, Button::Quiet, None, false, cx, move |s, _| {
                        s.select_model(preset)
                    })
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
                        .child(if self.available(preset) {
                            format!("{model} · {}", tr("Téléchargé"))
                        } else if recommended {
                            format!("{model} · {}", tr("conseillé"))
                        } else {
                            model.to_owned()
                        }),
                ),
            );
        }
        if self.settings["whisper_repo"] == "Systran/faster-whisper-tiny" {
            presets = presets.child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(tr(
                "Ancien modèle tiny actif. Choisissez Rapide pour télécharger la version base.",
            )));
        }
        let mut local = col().child(label("Modèles déjà téléchargés"));
        for (key, name, folder, empty) in [
            (
                "whisper",
                "Dossier Faster-Whisper",
                true,
                "Aucun modèle de transcription sélectionné",
            ),
            (
                "segmentation",
                "Segmentation ONNX",
                false,
                "Segmentation facultative",
            ),
            (
                "embedding",
                "Empreintes ONNX",
                false,
                "Empreintes vocales facultatives",
            ),
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
                        .child(self.settings[key].as_str().unwrap_or(tr(empty)).to_owned()),
                );
        }
        let summary = col()
            .gap(px(12.))
            .child(self.package_row(
                "Résumé contextualisé",
                "Qwen3 Instruct · 4B · GGUF · 2,50 Go",
                cx,
            ))
            .child(self.package_row("Résumé local", "Qwen2.5 · 3B · GGUF · 2,10 Go", cx))
            .child(window_button(
                "Choisir un fichier GGUF",
                Button::Quiet,
                None,
                false,
                cx,
                |s, w, cx| s.pick("gguf", false, w, cx),
            ))
            .when_some(self.settings["gguf"].as_str(), |d, path| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(path.to_owned()),
                )
            });
        let provider = col().child(label("Fournisseur distant facultatif"))
            .child(input(cx,&self.endpoint,"endpoint","URL HTTPS du fournisseur"))
            .child(input(cx,&self.remote_model,"remote_model","Nom du modèle distant"))
            .child(div().text_color(rgb(p.muted)).child(tr("Dans le Gestionnaire d’identifiants Windows, créez un identifiant générique Rikai/hôte avec votre clé comme mot de passe.")))
            .child(action("Enregistrer le fournisseur",false,cx,|s,cx|{
                let url=s.endpoint.read(cx).as_str().trim();
                anyhow::ensure!(url.is_empty() || (url.starts_with("https://") && !url.contains(['@','?','#'])),"URL HTTPS sans identifiants requise.");
                s.settings["endpoint"]=if url.is_empty(){Value::Null}else{json!(url)};
                s.settings["remote_model"]=json!(s.remote_model.read(cx).as_str());s.save_settings()
            }));
        col()
            .gap(px(18.))
            .child(presets)
            .child(summary)
            .child(local)
            .child(provider)
    }
    fn compute_settings(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let audio = col()
            .child(label("Nom de la piste microphone"))
            .child(input(cx, &self.self_name, "self-name", "Moi"))
            .child(action("Enregistrer mon nom", false, cx, |s, cx| {
                let name = s.self_name.read(cx).as_str().trim();
                anyhow::ensure!(!name.is_empty(), "Indiquez un nom.");
                s.settings["self_name"] = json!(name);
                s.save_settings()
            }));
        col()
            .gap(px(18.))
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
            .when(self.settings["device"] == "cuda", |d| {
                d.child(
                    col()
                        .when(!self.nvidia_ready, |c| {
                            c.child(action("Installer le moteur NVIDIA", false, cx, |s, _| {
                                s.page = Page::ConfirmDownload("NVIDIA CUDA");
                                Ok(())
                            }))
                        })
                        .child(if self.settings["whisper"].is_string() {
                            action("Vérifier CUDA avec ce modèle", false, cx, |s, _| {
                                s.check_cuda()
                            })
                        } else {
                            disabled_button("Vérifier CUDA avec ce modèle", p)
                        })
                        .child(toggle("Exiger le calcul sur GPU", "strict_cuda", self, cx)),
                )
            })
            .child(self.resource_settings(cx))
            .when(self.settings["language"] == "fr", |d| {
                d.child(
                    col()
                        .child(label("Vocabulaire français"))
                        .child(input(
                            cx,
                            &self.vocabulary,
                            "vocab",
                            "Noms et expressions, séparés par des virgules",
                        ))
                        .child(
                            if self.vocabulary.read(cx).as_str().trim()
                                == self.settings["vocabulary"].as_str().unwrap_or("")
                            {
                                disabled_button("Enregistrer le vocabulaire", p)
                            } else {
                                action("Enregistrer le vocabulaire", false, cx, |s, cx| {
                                    let terms = s.vocabulary.read(cx).as_str().trim();
                                    ensure!(
                                        terms.chars().count() <= 480,
                                        "Limitez le vocabulaire personnel à 480 caractères."
                                    );
                                    s.settings["vocabulary"] = json!(terms);
                                    s.save_settings()
                                })
                            },
                        ),
                )
            })
            .child(audio)
            .child(
                col()
                    .child(label("Cet ordinateur"))
                    .child(crate::i18n::message(&self.hardware)),
            )
            .child(
                col()
                    .child(label("Dossier des enregistrements"))
                    .child(self.root.display().to_string()),
            )
    }
}
fn disabled_button(text: &'static str, p: Palette) -> Stateful<Div> {
    div()
        .id(text)
        .role(Role::Button)
        .aria_disabled(true)
        .cursor_not_allowed()
        .flex()
        .items_center()
        .justify_center()
        .min_h(px(28.))
        .px(px(9.))
        .rounded(px(11.))
        .bg(rgb(p.control))
        .text_color(rgb(p.muted))
        .shadow(p.bevel())
        .opacity(0.5)
        .child(tr(text))
}
#[derive(IntoElement)]
struct CheckMark {
    on: bool,
    p: Palette,
}
impl RenderOnce for CheckMark {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let goal = if self.on { 1.0_f32 } else { 0.0 };
        let tr = w.use_keyed_transition("check-blend", cx, motion(150), |_, _| goal);
        if *tr.read_goal(cx) != goal {
            tr.update(cx, |v, _| *v = goal);
        }
        let t = if cx.reduce_motion() {
            goal
        } else {
            *tr.evaluate(w, cx)
        };
        let p = self.p;
        div()
            .id("check-box")
            .size(px(16.))
            .flex_shrink_0()
            .rounded(px(4.))
            .bg(rgb(if self.on { p.primary } else { p.base }))
            .shadow(p.bevel())
            .transitions(|s| s.bg(motion(150)))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .opacity(t)
                    .blur(px((1. - t) * 1.15))
                    .child(glyph(CHECK, 10. + 3. * t, p.on_primary)),
            )
    }
}
fn checkbox(text: String, on: bool, disabled: bool, p: Palette) -> Stateful<Div> {
    div()
        .id(SharedString::from(text.clone()))
        .role(Role::CheckBox)
        .aria_label(text.clone())
        .aria_toggled(on.into())
        .aria_disabled(disabled)
        .tab_stop(!disabled)
        .flex()
        .items_center()
        .gap_2()
        .min_h(px(28.))
        .rounded(px(6.))
        .when(disabled, |d| d.opacity(0.5).cursor_not_allowed())
        .when(!disabled, |d| {
            d.focusable()
                .tab_stop(true)
                .cursor_pointer()
                .focus_visible(move |d| d.bg(rgb(p.selected)))
        })
        .child(CheckMark { on, p })
        .child(text)
}

fn toggle(text: &'static str, key: &'static str, s: &Desk, cx: &Context<Desk>) -> impl IntoElement {
    let on = s.settings[key] == true;
    checkbox(tr(text).to_owned(), on, false, *cx.global::<Palette>())
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
            let levels = ctl.levels();
            for (i, name) in self.capture_names.iter().enumerate() {
                let level = levels
                    .get(&format!("piste-{}.wav", i + 1))
                    .copied()
                    .unwrap_or(0.);
                info = info.child(
                    col()
                        .gap(px(4.))
                        .child(raw_label(name.clone()))
                        .child(meter(level, *cx.global::<Palette>())),
                );
            }
            for notice in ctl.incidents().into_values() {
                info = info.child(crate::i18n::message(&notice));
            }
            if !self.stopping {
                info = info.child(action("Annuler l’enregistrement", false, cx, |s, _| {
                    s.ask(
                        "Abandonner l’enregistrement ?",
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
            let p = *cx.global::<Palette>();
            let elapsed = time(self.work_started.elapsed().as_millis() as u64);
            let mut body = col().child(label(self.stage.clone()));
            if let Some((done, total)) = self.transfer.filter(|(_, total)| *total > 0) {
                let percent = u128::from(done) * 100 / u128::from(total);
                let value = format!("{percent} %");
                body = body
                    .child(
                        bar(done as f32 / total as f32, p.primary, p)
                            .id("model-transfer")
                            .role(Role::ProgressIndicator)
                            .aria_label(tr("Téléchargement du modèle"))
                            .aria_value(value.clone()),
                    )
                    .child(
                        row()
                            .justify_between()
                            .child(div().text_color(rgb(p.muted)).child(crate::i18n::message(
                                &format!(
                                    "{:.1} / {:.1} Mio",
                                    done as f64 / 1048576.,
                                    total as f64 / 1048576.
                                ),
                            )))
                            .child(label(value)),
                    );
            }
            if let Some(count) = self.generated_chars {
                body = body.child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(
                    crate::i18n::message(&format!("Caractères générés pour ce bloc · {count}")),
                ));
            }
            let idle = self.last_progress.elapsed().as_secs();
            let timing = if idle >= 15 {
                format!("Temps écoulé {elapsed} · dernière réponse il y a {idle} s")
            } else {
                format!("Temps écoulé {elapsed}")
            };
            body = body.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(crate::i18n::message(&timing)),
            );
            if self
                .cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Acquire))
            {
                body = body.child(label("Annulation en cours…"));
            } else if page != Page::StorageChange {
                body = body.child(action("Annuler le traitement", false, cx, |s, _| {
                    if let Some(c) = &s.cancel {
                        c.store(true, Ordering::Release);
                    }
                    Ok(())
                }));
            }
            (
                if matches!(page, Page::Settings | Page::ConfirmDownload(_)) {
                    "Moteur de transcription"
                } else {
                    "Traitement"
                }
                .into(),
                body,
            )
        } else {
            match page {
                Page::Home => ("Espace de travail".into(), self.preview(cx)),
                Page::New => ("Enregistrer".into(), self.setup_view(cx)),
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
                            move |s, _| {
                                s.detail(id)?;
                                s.tab = 1;
                                if let Some(index) = s.segments.iter().position(|p| p.id == seg) {
                                    s.dialogues.scroll_to_reveal_item(index);
                                }
                                Ok(())
                            },
                        ));
                    }
                    (
                        "Mes enregistrements".into(),
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
                        d=d.child(tr("Aucun profil vocal enregistré. Créez un profil depuis une intervention transcrite, avec l’accord de la personne."));
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
                            }).id(("rename-profile",id as usize)).aria_label(crate::i18n::message(&format!("Renommer {name}"))))
                            .child(action("Supprimer",false,cx,move |s,_| {
                                s.ask("Supprimer ce profil vocal ?", "L’extrait autorisé, son empreinte et les suggestions associées seront supprimés.", "Supprimer", move |s| {
                                    s.db.delete_profile(id)?;
                                    s.profiles = s.db.profiles(false)?;
                                    Ok(())
                                })
                            }).id(("delete-profile",id as usize)).aria_label(crate::i18n::message(&format!("Supprimer {name}"))));
                        d = d.child(entry);
                    }
                    ("Intervenants".into(), d)
                }
                Page::StorageChange => ("Emplacement des composants".into(), col()),
                Page::Settings => ("Paramètres".into(), self.settings_view(cx)),
                Page::ConfirmDownload(preset) => (
                    if preset=="NVIDIA CUDA" {"Installer le moteur NVIDIA ?".into()} else {format!("{} · {}",tr("Télécharger"),tr(preset))},
                    col()
                        .child(tr(if preset=="NVIDIA CUDA" {"Téléchargement de 641 Mo depuis les distributions officielles NVIDIA sur PyPI. Prévoyez 2,5 Gio libres. Le pilote NVIDIA doit être installé."} else if preset=="Résumé contextualisé" {"Télécharger Qwen3 Instruct (2,50 Go) et l’utiliser pour les prochains résumés locaux ?"} else if preset=="Résumé local" {"Le résumé local utilise Qwen2.5. Télécharger le fichier GGUF de 2,10 Go sur cet ordinateur ?"} else if preset=="Traduction locale" {"La traduction de ce passage nécessite M2M100. Télécharger le modèle de 496 Mo pour traduire hors ligne ?"} else {"Le modèle de transcription sera téléchargé depuis Hugging Face et conservé sur cet ordinateur."}))
                        .child(row()
                            .child(action("Annuler", false, cx, |s, _| s.answer_download(false)))
                            .child(action("Télécharger", true, cx, |s, _| s.answer_download(true)))),
                ),
            }
        };
        (title, body)
    }
}

#[path = "live.rs"]
pub(super) mod live;
#[path = "window_frame.rs"]
pub(super) mod window_frame;
#[path = "workspace.rs"]
mod workspace;

#[path = "settings.rs"]
mod settings;
