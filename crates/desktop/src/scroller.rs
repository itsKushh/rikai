use super::*;
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
pub(super) enum Axis {
    Content(ScrollHandle),
    Rows(ListState),
}

impl Axis {
    fn y(&self) -> Pixels {
        match self {
            Self::Content(h) => h.offset().y,
            Self::Rows(h) => h.scroll_px_offset_for_scrollbar().y,
        }
    }
    fn max(&self) -> Pixels {
        match self {
            Self::Content(h) => h.max_offset().y,
            Self::Rows(h) => h.max_offset_for_scrollbar().y,
        }
    }
    fn bounds(&self) -> Bounds<Pixels> {
        match self {
            Self::Content(h) => h.bounds(),
            Self::Rows(h) => h.viewport_bounds(),
        }
    }
    fn set(&self, y: Pixels) {
        let position = point(px(0.), y.clamp(-self.max(), px(0.)));
        match self {
            Self::Content(h) => h.set_offset(position),
            Self::Rows(h) => h.set_offset_from_scrollbar(position),
        }
    }
}

#[derive(Default, Clone)]
pub(in crate::ui) struct Wheel {
    travel: Rc<Cell<Option<Travel>>>,
    scheduled: Rc<Cell<bool>>,
}
#[derive(Clone, Copy)]
struct Travel {
    from: Pixels,
    to: Pixels,
    written: Pixels,
    at: Instant,
}
impl Wheel {
    fn schedule(&self, axis: Axis, w: &mut Window) {
        if !self.scheduled.replace(true) {
            let wheel = self.clone();
            w.on_next_frame(move |w, _| {
                wheel.scheduled.set(false);
                wheel.frame(axis, w);
            });
        }
    }

    pub(in crate::ui) fn stop(&self) {
        self.travel.set(None);
    }

    fn frame(&self, axis: Axis, w: &mut Window) {
        let Some(mut travel) = self.travel.get() else {
            return;
        };
        // Keyboard navigation, dragging and follow-to-bottom take precedence.
        if (axis.y() - travel.written).abs() > px(1.) {
            self.travel.set(None);
            return;
        }
        let t = (travel.at.elapsed().as_secs_f32() / 0.17).min(1.);
        axis.set(travel.from + (travel.to - travel.from) * glide(t));
        travel.written = axis.y();
        w.refresh();
        if t < 1. {
            self.travel.set(Some(travel));
            self.schedule(axis, w);
        } else {
            self.travel.set(None);
        }
    }
    pub(super) fn listen(&self, axis: Axis, body: Div) -> Div {
        let wheel = self.clone();
        body.on_scroll_wheel(move |e, w, cx| {
            let ScrollDelta::Lines(lines) = e.delta else {
                wheel.travel.set(None);
                return;
            };
            if cx.reduce_motion() || lines.y == 0. {
                wheel.travel.set(None);
                return;
            }
            let old = wheel.travel.get();
            let y = axis.y();
            let to =
                (old.map_or(y, |v| v.to) + w.line_height() * lines.y).clamp(-axis.max(), px(0.));
            if to == y && old.is_none() {
                return;
            }
            wheel.travel.set(Some(Travel {
                from: y,
                to,
                written: y,
                at: Instant::now(),
            }));
            wheel.schedule(axis.clone(), w);
            cx.stop_propagation();
        })
    }
}

fn thumb(axis: &Axis) -> Option<(Pixels, Pixels)> {
    let h = axis.bounds().size.height;
    let max = axis.max();
    if max <= px(0.) || h <= px(0.) {
        return None;
    }
    let size = (h * (h / (h + max))).max(px(24.)).min(h);
    Some(((h - size) * (-axis.y() / max).clamp(0., 1.), size))
}
pub(super) fn rail(
    axis: Axis,
    name: &'static str,
    grab: Rc<Cell<Option<Pixels>>>,
    p: Palette,
) -> Stateful<Div> {
    let draw = axis.clone();
    let place = |a: &Axis, pointer: Pixels, grab: Pixels| {
        if let Some((_, size)) = thumb(a) {
            let bounds = a.bounds();
            a.set(
                -a.max()
                    * ((pointer - bounds.top() - grab) / (bounds.size.height - size).max(px(1.)))
                        .clamp(0., 1.),
            );
        }
    };
    let keys = axis.clone();
    let down = axis.clone();
    let drag = axis.clone();
    let on_down = grab.clone();
    let on_move = grab.clone();
    let on_up = grab.clone();
    div()
        .id(name)
        .role(Role::ScrollBar)
        .aria_label(tr(name))
        .focusable()
        .tab_stop(true)
        .w(px(12.))
        .h_full()
        .flex_shrink_0()
        .opacity(if grab.get().is_some() { 0.95 } else { 0.53 })
        .hover(|d| d.opacity(0.85))
        .focus_visible(|d| d.opacity(1.))
        .transitions(|t| t.opacity(motion(115)))
        .child(
            canvas(
                |_, _, _| (),
                move |b, _, w, _| {
                    if let Some((top, h)) = thumb(&draw) {
                        w.paint_quad(
                            PaintQuad::new(Bounds::new(
                                point(b.center().x - px(2.5), b.top() + top),
                                size(px(5.), h),
                            ))
                            .background(rgb(p.muted))
                            .corner_radii(px(2.5)),
                        );
                    }
                },
            )
            .size_full(),
        )
        .on_key_down(move |e, w, cx| {
            let y = keys.y();
            let h = keys.bounds().size.height;
            let target = match e.keystroke.key.as_str() {
                "home" => px(0.),
                "end" => -keys.max(),
                "pageup" => y + h,
                "pagedown" => y - h,
                "up" => y + px(32.),
                "down" => y - px(32.),
                _ => return,
            };
            keys.set(target);
            w.refresh();
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Left, move |e, w, _| {
            if let Some((top, h)) = thumb(&down) {
                let y = e.position.y - down.bounds().top();
                let grab = if (top..=top + h).contains(&y) {
                    y - top
                } else {
                    h * 0.5
                };
                on_down.set(Some(grab));
                if let Axis::Rows(list) = &down {
                    list.scrollbar_drag_started();
                }
                place(&down, e.position.y, grab);
                w.refresh();
            }
        })
        .on_mouse_move_all(move |e, phase, _, w, _| {
            if phase.bubble()
                && let Some(g) = on_move.get()
            {
                place(&drag, e.position.y, g);
                w.refresh();
            }
        })
        .on_mouse_up_all(move |_, phase, _, w, _| {
            if phase.bubble() && on_up.take().is_some() {
                if let Axis::Rows(list) = &axis {
                    list.scrollbar_drag_ended();
                }
                w.refresh();
            }
        })
}

#[derive(IntoElement)]
pub(super) struct ScrollArea {
    pub id: &'static str,
    pub body: Div,
    pub handle: Option<ScrollHandle>,
    pub limit: Option<f32>,
    pub rail: bool,
}
impl ScrollArea {
    pub fn new(id: &'static str, body: Div) -> Self {
        Self {
            id,
            body,
            handle: None,
            limit: None,
            rail: false,
        }
    }
    pub fn limited(mut self, h: f32) -> Self {
        self.limit = Some(h);
        self
    }
}
impl RenderOnce for ScrollArea {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let local = w.use_keyed_state(self.id, cx, |_, _| {
            (
                ScrollHandle::new(),
                Wheel::default(),
                Rc::new(Cell::new(None)),
            )
        });
        let (saved, wheel, grab) = local.read(cx).clone();
        let handle = self.handle.unwrap_or(saved);
        let axis = Axis::Content(handle.clone());
        let pane = div()
            .id(self.id)
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&handle)
            .child(wheel.listen(axis.clone(), self.body));
        div()
            .flex()
            .min_w_0()
            .min_h_0()
            .when(self.limit.is_none(), |d| d.flex_1().h_full())
            .when_some(self.limit, |d, h| d.max_h(px(h)))
            .child(pane.when_some(self.limit, |d, h| d.max_h(px(h))))
            .when(self.rail, |d| {
                d.child(rail(
                    axis,
                    "Parcourir les paramètres",
                    grab,
                    *cx.global::<Palette>(),
                ))
            })
    }
}
