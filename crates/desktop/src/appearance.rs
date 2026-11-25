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

pub(super) const TRANSLATE: &str = include_str!("../../../assets/nucleo/language.svg");
pub(super) const PLAY: &str = include_str!("../../../assets/nucleo/media-play.svg");
pub(super) const PAUSE: &str = include_str!("../../../assets/nucleo/media-pause.svg");
pub(super) const SAVE: &str = include_str!("../../../assets/nucleo/download.svg");
pub(super) const SESSION: &str = include_str!("../../../assets/nucleo/file-search.svg");
pub(super) const DESK: &str = include_str!("../../../assets/nucleo/folder-open.svg");
pub(super) const LIBRARY: &str = include_str!("../../../assets/nucleo/archive.svg");
pub(super) const VOICES: &str = include_str!("../../../assets/nucleo/users.svg");
pub(super) const MORE: &str = include_str!("../../../assets/nucleo/dots-vertical.svg");
pub(super) const EDIT: &str = include_str!("../../../assets/nucleo/pencil.svg");
pub(super) const HISTORY: &str =
    include_str!("../../../assets/nucleo/clock-rotate-anticlockwise.svg");
pub(super) const REMOTE: &str = include_str!("../../../assets/nucleo/cloud-upload.svg");
pub(super) const SLIDERS: &str = include_str!("../../../assets/nucleo/sliders-3-vertical.svg");
pub(super) const WAVE: &str = include_str!("../../../assets/nucleo/waveform-lines.svg");
pub(super) const CLOSE: &str = include_str!("../../../assets/nucleo/xmark.svg");
pub(super) const CHECK: &str = include_str!("../../../assets/nucleo/check.svg");
pub(super) const CHEVRON: &str = include_str!("../../../assets/nucleo/chevron-down.svg");
pub(super) const AUDIO: &str = include_str!("../../../assets/nucleo/music-note.svg");
pub(super) const MODEL: &str = include_str!("../../../assets/nucleo/brain-nodes.svg");
pub(super) const WINDOW_MIN: &str = include_str!("../../../assets/nucleo/minus.svg");
pub(super) const WINDOW_RESTORE: &str = include_str!("../../../assets/nucleo/duplicate.svg");
pub(super) const WINDOW_MAX: &str = include_str!("../../../assets/nucleo/shape-square.svg");
pub(super) fn glyph(data: &'static str, pixels: f32, color: u32) -> Svg {
    svg()
        .data(data.as_bytes())
        .size(px(pixels))
        .flex_shrink_0()
        .text_color(rgb(color))
}
pub(super) fn brand() -> Img {
    static IMAGE: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    img(IMAGE
        .get_or_init(|| {
            std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../../../assets/rikai-mini-icon.png").to_vec(),
            ))
        })
        .clone())
    .size(px(24.))
    .flex_shrink_0()
}

pub(super) fn interface_font() -> Font {
    static FACE: std::sync::OnceLock<Font> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        let mut face = font("Inter");
        face.fallbacks = Some(FontFallbacks::from_fonts(
            [
                "Segoe UI",
                "Yu Gothic UI",
                "Microsoft YaHei UI",
                "Malgun Gothic",
                "Rikai Script UI",
                "Noto Sans",
                "Noto Sans Arabic",
                "Noto Sans Ethiopic",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ));
        face
    })
    .clone()
}
