use super::*;

impl Desk {
    pub(super) fn settings_tabs(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut tabs =
            workspace::strip(p, "settings-pill", self.section, &self.tab_geometry[3]).w_full();
        for (i, name) in [
            "Modèles",
            "Dépendances",
            "Interface",
            "Calcul",
            "Mises à jour",
        ]
        .into_iter()
        .enumerate()
        {
            tabs = tabs.child(
                button(
                    name,
                    if i == self.section {
                        Button::Tab
                    } else {
                        Button::IdleTab
                    },
                    None,
                    false,
                    cx,
                    move |s, _| {
                        s.section = i;
                        s.menu = None;
                        Ok(())
                    },
                )
                .flex_1()
                .px(px(4.))
                .text_size(px(11.)),
            );
        }
        div().px(px(8.)).py(px(7.)).child(tabs)
    }
    pub(super) fn package_row(
        &self,
        preset: &'static str,
        description: &'static str,
        cx: &Context<Self>,
    ) -> Div {
        let p = *cx.global::<Palette>();
        col()
            .child(label(preset))
            .child(div().text_color(rgb(p.muted)).child(tr(description)))
            .child(if self.available(preset) {
                disabled_button("Téléchargé", p)
            } else {
                action("Télécharger", false, cx, move |s, _| {
                    s.page = Page::ConfirmDownload(preset);
                    Ok(())
                })
            })
            .when(
                self.available(preset)
                    && self.settings[if preset == "Résumé local" {
                        "gguf"
                    } else {
                        "translation"
                    }] != self.installed[preset]["path"],
                |d| {
                    d.child(action("Utiliser ce modèle", false, cx, move |s, _| {
                        s.settings[if preset == "Résumé local" {
                            "gguf"
                        } else {
                            "translation"
                        }] = s.installed[preset]["path"].clone();
                        s.save_settings()
                    }))
                },
            )
    }
    pub(super) fn locale_control(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let selected = self.settings["locale"].as_str().unwrap_or("");
        let choices = std::iter::once(("", tr("Langue du système")))
            .chain(crate::i18n::LOCALES.iter().copied());
        let current = choices
            .clone()
            .find(|(code, _)| *code == selected)
            .map(|v| v.1)
            .unwrap_or(tr("Langue du système"));
        let open = self.menu == Some(Preference::Locale);
        let mut field = col()
            .gap(px(4.))
            .child(label("Langue de l’interface"))
            .child(
                action(current, false, cx, move |s, _| {
                    s.menu = if open { None } else { Some(Preference::Locale) };
                    Ok(())
                })
                .id("locale-select")
                .justify_between()
                .child(glyph(CHEVRON, 14., p.muted)),
            );
        if open {
            let mut menu = col()
                .p(px(3.))
                .gap(px(2.))
                .rounded(px(12.))
                .bg(rgb(p.raised))
                .border_1()
                .border_color(rgb(p.edge));
            for (index, (code, name)) in choices.enumerate() {
                menu = menu.child(
                    button(
                        name,
                        if code == selected {
                            Button::Selected
                        } else {
                            Button::Quiet
                        },
                        None,
                        false,
                        cx,
                        move |s, _| {
                            crate::i18n::select(Some(code));
                            s.settings["locale"] = json!(code);
                            s.menu = None;
                            s.save_settings()
                        },
                    )
                    .id(("locale", index))
                    .rounded(px(9.))
                    .justify_between()
                    .when(code == selected, |d| d.child(glyph(CHECK, 14., p.text))),
                );
            }
            field = field.child(menu.with_animation("locale-menu", reveal(cx, 140), |d, t| {
                d.relative().top(px(-3. * (1. - t))).opacity(t)
            }));
        }
        field
    }
    pub(super) fn update_settings(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let waiting = self.update_checking;
        let auto = self.settings["updates_auto"] != false;
        let info = match self.update_status.as_str() {
            "current" => "Rikai est à jour.",
            "disconnected" | "cancelled" | "" => {
                "Connectez GitHub pour consulter les versions privées."
            }
            "available" => "Une mise à jour est disponible.",
            "ready" => "Mise à jour prête.",
            other => other,
        };
        col().child(label("Mises à jour privées"))
            .child(checkbox(tr("Vérifier au démarrage").into(),auto,false,p).on_click(cx.listener(move|s,_,_,cx|{
                s.settings["updates_auto"]=json!(!auto);if let Err(e)=s.save_settings(){s.message=e.to_string();}cx.notify();
            })))
            .child(div().text_color(rgb(p.muted)).child(crate::i18n::message(info)))
            .child(div().text_color(rgb(p.muted)).child(tr("Dans GitHub, sélectionnez le dépôt kpiche et donnez au jeton un accès en lecture au contenu. Windows conserve le jeton sur cet ordinateur.")))
            .child(if waiting{disabled_button("Connexion en cours…",p)}else{action("Connecter GitHub",false,cx,|s,_|{s.update_request("connect_github");Ok(())})})
            .child(if waiting{disabled_button("Vérifier maintenant",p)}else{action("Vérifier maintenant",false,cx,|s,_|{s.update_request("check_update");Ok(())})})
            .when(!waiting && !matches!(self.update_status.as_str(),"disconnected"|"cancelled"|""),|d|d.child(button("Déconnecter GitHub",Button::Quiet,None,false,cx,|s,_|{s.update_request("disconnect_github");Ok(())})))
    }
    pub(super) fn release_modal(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let Some(info) = &self.update_info else {
            return div();
        };
        let animation = if self.update_checking || self.update_ready {
            "release-transfer"
        } else {
            "release-offer"
        };
        let title = if self.update_ready {
            "Mise à jour prête"
        } else if self.update_checking {
            "Téléchargement de la mise à jour"
        } else {
            "Mise à jour disponible"
        };
        let mut content =
            col()
                .p_4()
                .gap_3()
                .child(label(title))
                .child(div().text_color(rgb(p.muted)).child(format!(
                    "Rikai {} → {}",
                    env!("CARGO_PKG_VERSION"),
                    info["version"].as_str().unwrap_or("")
                )));
        if self.update_checking {
            let (n, total) = self
                .transfer
                .unwrap_or((0, info["size"].as_u64().unwrap_or(1)));
            content = content
                .child(bar(n as f32 / total.max(1) as f32, p.primary, p))
                .child(format!("{} %", n.saturating_mul(100) / total.max(1)));
        } else if self.update_ready {
            content = content.child(
                row()
                    .child(action("Redémarrer Rikai", true, cx, |s, _| {
                        s.update_request("restart_update");
                        Ok(())
                    }))
                    .child(action("Installer et fermer", false, cx, |s, _| {
                        s.update_request("close_update");
                        Ok(())
                    })),
            );
        } else {
            let mut notes = col().gap_1();
            for line in info["notes"]
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
            {
                notes = if line.starts_with('#') {
                    notes.child(label(line.trim_start_matches('#').trim().to_string()).mt_2())
                } else {
                    notes.child(
                        div().flex().gap_2().child("•").child(
                            div().flex_1().min_w_0().child(
                                line.trim_start_matches("- ")
                                    .replace("**", "")
                                    .replace('`', ""),
                            ),
                        ),
                    )
                };
            }
            content = content
                .child(
                    div()
                        .id("release-notes")
                        .max_h(px(260.))
                        .overflow_y_scroll()
                        .child(notes),
                )
                .child(
                    row()
                        .child(action("Mettre à jour", true, cx, |s, _| {
                            s.update_request("download_update");
                            Ok(())
                        }))
                        .child(button(
                            "Plus tard",
                            Button::Quiet,
                            None,
                            false,
                            cx,
                            |s, _| {
                                s.update_info = None;
                                Ok(())
                            },
                        )),
                );
        }
        if !matches!(
            self.update_status.as_str(),
            "available" | "ready" | "applying"
        ) {
            content = content.child(crate::i18n::message(&self.update_status));
        }
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(black().opacity(0.66))
            .child(
                p.panel()
                    .id("release-dialog")
                    .track_focus(&self.release_focus)
                    .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                        if e.keystroke.key == "escape" && !s.update_ready && !s.update_checking {
                            s.update_info = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .role(Role::Dialog)
                    .aria_modal(true)
                    .aria_label(tr(title))
                    .w(px(430.))
                    .max_w(relative(0.9))
                    .child(content)
                    .with_animation(animation, reveal(cx, 200), |d, t| {
                        d.relative().top(px(7. * (1. - t))).opacity(t)
                    }),
            )
    }
}
