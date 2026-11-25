use super::*;

pub(in crate::ui) fn options(
    title: impl Into<SharedString>,
    initial: Size<Pixels>,
    minimum: Size<Pixels>,
    cx: &App,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, initial, cx))),
        window_min_size: Some(minimum),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(super) fn compact(window: &Window) -> bool {
    window.viewport_size().width < px(900.)
}

pub(super) fn surface(p: Palette) -> Div {
    div()
        .size_full()
        .relative()
        .flex()
        .flex_col()
        .bg(rgb(p.base))
        .text_color(rgb(p.text))
        .font(interface_font())
        .text_size(px(12.))
}

pub(super) fn header(p: Palette, leading: Div, draggable: bool) -> Div {
    let drag_area = || {
        div().h_full().when(draggable, |d| {
            d.window_control_area(WindowControlArea::Drag)
        })
    };
    div()
        .flex()
        .items_center()
        .h(px(54.))
        .py(px(6.))
        .flex_shrink_0()
        .gap(px(10.))
        .pl(px(16.))
        .child(drag_area().flex().items_center().pr_2().child(brand()))
        .child(
            div()
                .w(px(1.))
                .h(px(15.))
                .flex_shrink_0()
                .bg(rgb(p.edge).opacity(0.53)),
        )
        .child(leading)
        .child(drag_area().flex_1())
}

pub(super) fn controls(
    window: &Window,
    p: Palette,
    native_hit_test: bool,
    close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let caption = |name: &'static str, icon, area| {
        div()
            .id(name)
            .aria_label(tr(name))
            .role(Role::Button)
            .focusable()
            .tab_stop(true)
            .h_full()
            .w(px(if compact(window) { 32. } else { 40. }))
            .flex()
            .items_center()
            .justify_center()
            .when(native_hit_test, |s| s.window_control_area(area))
            .opacity(0.60)
            .hover(|s| s.opacity(1.))
            .focus_visible(|s| s.opacity(1.))
            .transitions(|t| t.opacity(motion(115)))
            .child(glyph(
                icon,
                if icon == WINDOW_MAX { 12. } else { 14. },
                p.text,
            ))
    };
    let (zoom_label, zoom_icon) = if window.is_maximized() {
        ("Restaurer", WINDOW_RESTORE)
    } else {
        ("Agrandir", WINDOW_MAX)
    };
    div()
        .flex()
        .flex_shrink_0()
        .h_full()
        .items_center()
        .child(
            caption("Réduire", WINDOW_MIN, WindowControlArea::Min)
                .on_click(|_, w, _| w.minimize_window()),
        )
        .child(
            caption(zoom_label, zoom_icon, WindowControlArea::Max)
                .on_click(|_, w, _| w.zoom_window()),
        )
        .child(caption("Fermer", CLOSE, WindowControlArea::Close).on_click(close))
}
