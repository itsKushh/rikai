use gpui::{prelude::*, *};

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub base: u32,
    pub surface: u32,
    pub raised: u32,
    pub control: u32,
    pub selected: u32,
    pub text: u32,
    pub muted: u32,
    pub primary: u32,
    pub on_primary: u32,
    pub edge: u32,
    pub level: u32,
}

impl Global for Palette {}

impl Palette {
    pub fn new(light: bool) -> Self {
        if light {
            Self {
                base: 0xe9e9eb,
                surface: 0xf6f6f7,
                raised: 0xfbfbfc,
                control: 0xe9e9ec,
                selected: 0xffffff,
                text: 0x252528,
                muted: 0x64646c,
                primary: 0x29292d,
                on_primary: 0xf7f7f8,
                edge: 0xd1d1d6,
                level: 0x28734f,
            }
        } else {
            Self {
                base: 0x111113,
                surface: 0x1d1d20,
                raised: 0x242427,
                control: 0x2c2c30,
                selected: 0x36363b,
                text: 0xededf0,
                muted: 0xa5a5ad,
                primary: 0xe5e5e9,
                on_primary: 0x1c1c20,
                edge: 0x39393d,
                level: 0x83c89b,
            }
        }
    }

    pub fn bevel(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow::new(
                px(0.),
                px(1.),
                black().opacity(if self.base == 0x111113 { 0.27 } else { 0.12 }),
            )
            .blur_radius(px(2.)),
            BoxShadow::new(
                px(0.),
                px(-1.),
                black().opacity(if self.base == 0x111113 { 0.20 } else { 0.09 }),
            )
            .inset(),
            BoxShadow::new(
                px(0.),
                px(1.),
                white().opacity(if self.base == 0x111113 { 0.10 } else { 0.66 }),
            )
            .inset(),
        ]
    }
    pub fn pressed(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow::new(px(0.), px(2.), black().opacity(0.34))
                .blur_radius(px(2.))
                .inset(),
        ]
    }
    pub fn panel(self) -> Div {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .rounded(px(16.))
            .border_1()
            .border_color(rgb(self.edge))
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgb(self.raised), 0.),
                linear_color_stop(rgb(self.surface), 1.),
            ))
            .shadow(self.bevel())
    }
}

// Petits pictogrammes monochromes dessinés sur la même grille de 24 unités.
pub(super) const PLAY: &str = "M7 4.5 20 12 7 19.5Z";
pub(super) const PAUSE: &str = "M7 5v14m9-14v14";
pub(super) const SAVE: &str = "M12 3v12m-5-5 5 5 5-5M5 16v4h14v-4";
pub(super) const WAVE: &str = "M3 10v4m4-8v12m5-16v20m5-17v14m4-10v6";
pub(super) const MIC: &str =
    "M9 6a3 3 0 0 1 6 0v6a3 3 0 0 1-6 0ZM5 12a7 7 0 0 0 14 0m-7 7v3m-4 0h8";
pub(super) const DESK: &str = "M3 5h18v14H3zM3 10h18m-6 0v9";
pub(super) const LIBRARY: &str = "M4 6h16M4 12h16M4 18h16";
pub(super) const VOICES: &str =
    "M9 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm-7 8a7 7 0 0 1 14 0m0-15a4 4 0 0 1 0 7m2 2a6 6 0 0 1 4 6";
pub(super) const SLIDERS: &str = "M5 3v7m0 5v6m7-18v12m0 4v2m7-18v2m0 4v12M2 10h6m1 5h6m1-9h6";
pub(super) const PLUS: &str = "M12 5v14M5 12h14";
pub(super) const CLOSE: &str = "m5 5 14 14M19 5 5 19";
pub(super) const CHECK: &str = "m5 12 4 4 10-10";
pub(super) const CHEVRON: &str = "m7 10 5 5 5-5";
pub(super) const AUDIO: &str = "M3 10h4l5-5v14l-5-5H3zm13-2a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14";
pub(super) const MODEL: &str = "M6 5h12v16l-6-4-6 4z";

pub(super) fn glyph(path: &str, pixels: f32, color: u32) -> Svg {
    let data = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{path}" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
    );
    svg()
        .data(data.as_bytes())
        .size(px(pixels))
        .flex_shrink_0()
        .text_color(rgb(color))
}
