use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(in crate::ui) enum Budget {
    Threads,
    Ram,
    Gpu,
}

impl Budget {
    fn key(self) -> &'static str {
        match self {
            Self::Threads => "cpu_threads",
            Self::Ram => "ram_gib",
            Self::Gpu => "gpu_duty",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Threads => "Threads CPU",
            Self::Ram => "Plafond de RAM",
            Self::Gpu => "Temps de calcul GPU",
        }
    }
    fn range(self, s: &Desk) -> (u64, u64) {
        match self {
            Self::Threads => (0, s.load.threads.max(1)),
            Self::Ram => (0, s.load.total_gib.max(2)),
            Self::Gpu => (10, 100),
        }
    }
    fn saved(self, s: &Desk) -> u64 {
        s.settings[self.key()]
            .as_u64()
            .unwrap_or(if self == Self::Gpu { 100 } else { 0 })
    }
    fn snap(self, n: u64) -> u64 {
        match self {
            Self::Gpu => n.div_ceil(5) * 5,
            _ => n,
        }
    }
    fn value(self, s: &Desk) -> String {
        let n = self.saved(s);
        match self {
            Self::Gpu => format!("{n} %"),
            Self::Ram if n == 0 => "Géré par Windows".into(),
            _ if n == 0 => "Auto".into(),
            Self::Ram => format!("{n} Gio"),
            _ => n.to_string(),
        }
    }
}

impl Desk {
    fn adjust_budget(&mut self, b: Budget, x: Pixels, bounds: Bounds<Pixels>) {
        if bounds.size.width <= px(0.) {
            return;
        }
        let f = ((x - bounds.origin.x) / bounds.size.width).clamp(0., 1.);
        let (lo, hi) = b.range(self);
        self.settings[b.key()] = json!(
            b.snap(lo + ((hi - lo) as f32 * f).round() as u64)
                .clamp(lo, hi)
        );
    }
    fn budget(&self, b: Budget, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let (lo, hi) = b.range(self);
        let n = b.saved(self).clamp(lo, hi);
        let f = (n - lo) as f32 / (hi - lo).max(1) as f32;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measure = bounds.clone();
        let release =
            move |s: &mut Desk, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Desk>| {
                if s.budget_drag.is_some_and(|(active, _)| active == b) {
                    s.budget_drag = None;
                    if let Err(e) = s.save_settings() {
                        s.message = e.to_string();
                    }
                    cx.notify();
                }
            };
        let track = div()
            .id(b.key())
            .relative()
            .h(px(24.))
            .w_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(Role::Slider)
            .aria_label(b.name())
            .aria_value(b.value(self))
            .rounded(px(5.))
            .focus_visible(move |d| d.bg(rgb(p.surface)).border_1().border_color(rgb(p.muted)))
            .on_prepaint(move |layout, _, _| measure.set(layout.bounds))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, e: &MouseDownEvent, _, cx| {
                    let area = bounds.get();
                    s.budget_drag = Some((b, area));
                    s.adjust_budget(b, e.position.x, area);
                    cx.notify();
                }),
            )
            .on_mouse_move_all({
                let drag = cx.listener(move |s, e: &MouseMoveEvent, _, cx| {
                    if let Some((active, area)) = s.budget_drag
                        && active == b
                    {
                        s.adjust_budget(b, e.position.x, area);
                        cx.notify();
                    }
                });
                move |e, phase, _, w, cx| {
                    if phase.bubble() {
                        drag(e, w, cx)
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, cx.listener(release))
            .on_mouse_up_out(MouseButton::Left, cx.listener(release))
            .on_key_down(cx.listener(move |s, e: &KeyDownEvent, _, cx| {
                let step = if b == Budget::Gpu { 5 } else { 1 };
                let value = match e.keystroke.key.as_str() {
                    "left" | "down" => n.saturating_sub(step),
                    "right" | "up" => n + step,
                    "home" => lo,
                    "end" => hi,
                    _ => return,
                };
                s.settings[b.key()] = json!(value.clamp(lo, hi));
                if let Err(e) = s.save_settings() {
                    s.message = e.to_string();
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded_full()
                    .bg(rgb(p.control))
                    .child(
                        div()
                            .id("resource-fill")
                            .w(relative(f))
                            .h_full()
                            .rounded_full()
                            .bg(rgb(p.muted))
                            .transitions(|t| t.w(motion(75))),
                    ),
            )
            .child(
                div()
                    .id("resource-thumb")
                    .absolute()
                    .left(relative(f))
                    .transitions(|t| t.left(motion(75)))
                    .ml(px(-5.))
                    .size(px(10.))
                    .rounded_full()
                    .bg(rgb(p.primary))
                    .shadow(p.bevel()),
            );
        col()
            .gap(px(1.))
            .child(
                row()
                    .justify_between()
                    .child(b.name())
                    .child(div().text_color(rgb(p.muted)).child(b.value(self))),
            )
            .child(track)
    }

    pub(super) fn resource_settings(&self, cx: &Context<Self>) -> Div {
        let p = *cx.global::<Palette>();
        let mut panel = col().gap_2().child(label("Ressources de transcription"));
        if self.load.threads > 0 {
            panel = panel.child(div().text_color(rgb(p.muted)).child(format!(
                "CPU {:.0} % · RAM libre {:.1} / {} Gio",
                self.load.cpu, self.load.free_gib, self.load.total_gib
            )));
        }
        panel = panel
            .child(self.budget(Budget::Threads, cx))
            .child(self.budget(Budget::Ram, cx));
        if self.settings["device"] == "cuda" {
            let gpu = self
                .load
                .gpu
                .clone()
                .unwrap_or_else(|| "Mesure NVIDIA indisponible".into());
            let help =
                "Le GPU alterne calcul et pauses entre les séquences. 100 % privilégie la vitesse.";
            panel = panel
                .child(div().text_color(rgb(p.muted)).child(gpu))
                .child(self.budget(Budget::Gpu, cx))
                .child(div().text_color(rgb(p.muted)).child(help));
        }
        panel
    }
}
