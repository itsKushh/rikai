use super::scroller::ScrollArea;
use super::*;

#[derive(Default)]
pub(in crate::ui) struct Sidebar {
    selection: usize,
    origins: Vec<f32>,
    since: Option<Instant>,
    direction: f32,
}
impl Sidebar {
    fn shift(&self, i: usize, current: usize, t: f32) -> f32 {
        if self.selection != current || self.since.is_none() || t >= 1. {
            return if current == i { 0. } else { 1. };
        }
        let from = self.origins[i];
        if i == current {
            let enter = if from.abs() < 1. {
                t
            } else {
                ((t - 0.42) / 0.58).clamp(0., 1.)
            };
            from * (1. - glide(enter))
        } else if from.abs() >= 1. {
            from
        } else {
            from + (self.direction - from) * glide((t / 0.42).min(1.))
        }
    }
    fn phase(&self) -> f32 {
        self.since
            .map_or(1., |s| (s.elapsed().as_secs_f32() / 0.31).min(1.))
    }
    pub(super) fn select(&mut self, from: usize, to: usize, count: usize) {
        if from == to {
            return;
        }
        let t = self.phase();
        let mut origins: Vec<_> = (0..count).map(|i| self.shift(i, from, t)).collect();
        self.direction = if to > from { 1. } else { -1. };
        if origins[to].abs() >= 1. {
            origins[to] = -self.direction;
        }
        self.origins = origins;
        self.selection = to;
        self.since = Some(Instant::now());
    }
    pub(super) fn render<V: 'static>(
        &self,
        names: &[&'static str],
        current: usize,
        w: &Window,
        cx: &Context<V>,
        select: fn(&mut V, usize),
    ) -> Div {
        let p = *cx.global::<Palette>();
        let t = if cx.reduce_motion() { 1. } else { self.phase() };
        if t < 1. {
            w.request_animation_frame();
        }
        let mut side = col()
            .gap(px(3.))
            .w(if w.viewport_size().width < px(620.) {
                px(114.)
            } else {
                px(166.)
            })
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(p.edge))
            .p(px(8.));
        for (i, &name) in names.iter().enumerate() {
            let shift = self.shift(i, current, t);
            side = side.child(
                div()
                    .relative()
                    .overflow_hidden()
                    .rounded(px(11.))
                    .flex_shrink_0()
                    .when(shift.abs() < 1., |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(shift))
                                .w_full()
                                .rounded(px(11.))
                                .bg(rgb(p.selected)),
                        )
                    })
                    .child(
                        control(
                            name,
                            if i == current {
                                Button::Tab
                            } else {
                                Button::IdleTab
                            },
                            None,
                            false,
                            cx,
                        )
                        .id(("settings-category", i))
                        .w_full()
                        .justify_start()
                        .px_2()
                        .py(px(6.))
                        .min_h(px(32.))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            select(s, i);
                            cx.notify();
                        })),
                    ),
            );
        }
        side
    }
}
pub(super) fn visibility(open: bool, w: &mut Window, cx: &mut App) -> f32 {
    let goal = if open { 1. } else { 0. };
    let transition = w.use_keyed_transition("drawer-visibility", cx, motion(290), |_, _| 0_f32);
    if *transition.read_goal(cx) != goal {
        transition.update(cx, |v, _| *v = goal);
    }
    if cx.reduce_motion() {
        goal
    } else {
        *transition.evaluate(w, cx)
    }
}

pub(super) fn sheet(
    title: String,
    close: Option<Stateful<Div>>,
    side: Option<Div>,
    body: Div,
    handle: ScrollHandle,
    p: Palette,
) -> Stateful<Div> {
    let mut area = ScrollArea::new("settings-scroll", body.p_4().gap(px(16.)));
    area.handle = Some(handle);
    area.rail = true;
    p.panel()
        .id("settings-sheet")
        .w(relative(0.94))
        .h(relative(0.92))
        .max_w(px(1000.))
        .max_h(px(720.))
        .occlude()
        .role(Role::Dialog)
        .aria_modal(true)
        .aria_label(title.clone())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .min_h(px(42.))
                .flex_shrink_0()
                .px_4()
                .border_b_1()
                .border_color(rgb(p.edge))
                .child(label(title))
                .children(close),
        )
        .child(div().flex().flex_1().min_h_0().children(side).child(area))
        .child(
            div()
                .flex_shrink_0()
                .px_4()
                .py_2()
                .text_right()
                .text_size(px(11.))
                .text_color(rgb(p.muted))
                .child(concat!("Rikai v", env!("CARGO_PKG_VERSION"))),
        )
}

pub(super) fn overlay(v: f32, sheet: Stateful<Div>, backdrop: Stateful<Div>) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(12.))
        .child(
            backdrop
                .absolute()
                .inset_0()
                .occlude()
                .bg(black().opacity(v * 0.67))
                .backdrop_blur(px(4. * v)),
        )
        .child(sheet.relative().top(px(8. * (1. - v))).opacity(v))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum SourceMenu {
    Apps,
    Mic,
}
impl SourceMenu {
    fn mic(self) -> bool {
        self == Self::Mic
    }
    pub(super) fn toggle(self, selected: &mut HashSet<Source>, src: Source) {
        if !selected.remove(&src) {
            if self.mic() {
                selected.retain(|s| !matches!(s, Source::Microphone { .. }));
            }
            selected.insert(src);
        }
    }
}

pub(super) fn source_field<V: 'static>(
    kind: SourceMenu,
    open: bool,
    inputs: &[Input],
    selected: &HashSet<Source>,
    scanning: bool,
    locked: bool,
    detail: impl Fn(&Input, bool) -> Option<String>,
    cx: &Context<V>,
    toggle: fn(&mut V, SourceMenu),
    pick: fn(&mut V, SourceMenu, Source),
) -> Div {
    let mic = kind.mic();
    let name = if mic { "Microphone" } else { "Applications" };
    let filtered: Vec<_> = inputs
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.source, Source::Microphone { .. }) == mic)
        .collect();
    let chosen: Vec<_> = filtered
        .iter()
        .filter(|(_, i)| selected.contains(&i.source))
        .map(|(_, i)| i.name.as_str())
        .collect();
    let value = if chosen.len() > 1 {
        format!("{} +{}", chosen[0], chosen.len() - 1)
    } else {
        chosen
            .first()
            .copied()
            .unwrap_or(tr("Sélectionner"))
            .to_owned()
    };
    let p = *cx.global::<Palette>();
    let trigger = if locked {
        div()
            .id(name)
            .role(Role::Button)
            .aria_label(tr(name))
            .cursor_not_allowed()
            .flex()
            .items_center()
            .min_h(px(28.))
            .px(px(9.))
            .rounded(px(11.))
            .bg(rgb(p.control))
            .text_color(rgb(p.muted))
            .opacity(0.55)
    } else {
        control(name, Button::Secondary, None, true, cx)
    }
    .id(("source-picker", mic as usize))
    .w_full()
    .justify_between()
    .aria_expanded(open && !locked)
    .aria_disabled(locked)
    .tab_stop(!locked)
    .child(div().min_w_0().truncate().child(value))
    .child(glyph(CHEVRON, 14., p.muted))
    .when(!locked, |d| {
        d.on_click(cx.listener(move |s, _, _, cx| {
            toggle(s, kind);
            cx.notify();
        }))
    });
    let mut field = col().gap(px(4.)).child(label(name)).child(trigger);
    if open && !locked {
        let mut options = col().gap(px(2.));
        if filtered.is_empty() {
            options = options.child(div().p_2().text_color(rgb(p.muted)).child(tr(if scanning {
                "Détection en cours…"
            } else if mic {
                "Aucune entrée disponible."
            } else {
                "Aucune session détectée. Lancez un son dans l’application."
            })));
        }
        for (i, input) in filtered {
            let source = input.source.clone();
            let title = match &source {
                Source::Application { pid, .. } => format!("{} · PID {pid}", input.name),
                _ => input.name.clone(),
            };
            options = options.child(
                checkbox(title, selected.contains(&source), false, p)
                    .when_some(detail(input, selected.contains(&source)), |d, value| {
                        d.child(div().flex_1())
                            .child(div().text_color(rgb(p.muted)).child(value))
                    })
                    .id(("source-option", i))
                    .px_2()
                    .min_h(px(32.))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        pick(s, kind, source.clone());
                        cx.notify();
                    })),
            );
        }
        let menu = div()
            .flex()
            .flex_col()
            .p(px(3.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(p.edge))
            .bg(rgb(p.raised))
            .child(
                ScrollArea::new(
                    if mic {
                        "microphone-options"
                    } else {
                        "application-options"
                    },
                    options,
                )
                .limited(168.),
            );
        field = field.child(menu.with_animation(
            ("source-open", mic as usize),
            reveal(cx, 145),
            |d, t| d.relative().top(px(-3. * (1. - t))).opacity(t),
        ));
    }
    div().child(
        field
            .id(("source-field", mic as usize))
            .when(open && !locked, |d| {
                d.on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |s, _: &MouseUpEvent, _, cx| {
                        toggle(s, kind);
                        cx.notify();
                    }),
                )
            }),
    )
}
