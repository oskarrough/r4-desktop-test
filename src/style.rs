//! The look: one grid, three type sizes, a dark palette with one accent.
//! Every edge sits on a multiple of `UNIT`; text starts `MARGIN` in from the window's sides.
use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
use fastframe_fonts::{LocalFont, Weight};
use std::sync::Arc;

pub const UNIT: f32 = 4.0;
/// Left and right of everything: text, bars, the last button's glyph.
pub const MARGIN: f32 = 4.0 * UNIT;

/// Times, and the line under a title.
pub const SMALL: f32 = 11.0;
/// Rows, headers, the transport's title.
pub const BODY: f32 = 14.0;
/// The now-playing title.
pub const LARGE: f32 = 18.0;

/// Which typeface leads. Flip this to go back to Radio Canada.
const TYPEFACE: Typeface = Typeface::Quattro;

/// The leading face's capital height, as a share of the font size.
pub const CAP: f32 = TYPEFACE.cap();

pub const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x11);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x28);
pub const HOVER: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1b);
pub const SELECTED: Color32 = Color32::from_rgb(0x26, 0x26, 0x24);
pub const TEXT: Color32 = Color32::from_rgb(0xed, 0xed, 0xeb);
pub const SECONDARY: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x88);
pub const DISABLED: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x47);
/// Only for what is live: progress, the playing dot, the channel link.
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x5a, 0x1f);

#[allow(dead_code)]
enum Typeface {
    /// iA Writer Quattro: static Regular and Bold files.
    Quattro,
    /// Radio Canada: one variable file.
    RadioCanada,
}

const QUATTRO_REGULAR: &[u8] = include_bytes!("../assets/fonts/iAWriterQuattroS-Regular.ttf");
const QUATTRO_BOLD: &[u8] = include_bytes!("../assets/fonts/iAWriterQuattroS-Bold.ttf");
const RADIO_CANADA: &[u8] = include_bytes!("../assets/fonts/RadioCanada.ttf");
const NOTO_EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoEmoji.ttf");

impl Typeface {
    const fn cap(&self) -> f32 {
        match self {
            Self::Quattro => 0.70,
            Self::RadioCanada => 0.69,
        }
    }

    /// Lead the interface families with this face.
    fn lead(&self, setup: fastframe_fonts::FontSetup) -> fastframe_fonts::FontSetup {
        match self {
            // Static files have no weight axis, so Regular and Bold are added in `install`.
            Self::Quattro => setup,
            Self::RadioCanada => match LocalFont::from_bytes(RADIO_CANADA.to_vec(), 0) {
                Ok(face) => setup.local_font(face),
                Err(_) => setup,
            },
        }
    }
}

pub fn font(size: f32, weight: Weight) -> FontId {
    weight.font_id(size)
}

pub fn install(ctx: &egui::Context) {
    // The typeface leads, Inter stays behind it for scripts it lacks, then the
    // monochrome emoji face, which colour emoji paint over where the system has them.
    let mut setup = fastframe_fonts::FontSetup::default()
        .weights(&[Weight::SemiBold])
        .companion(
            "noto_emoji",
            Arc::new(egui::FontData::from_static(NOTO_EMOJI)),
        );
    setup = TYPEFACE.lead(setup);
    let mut fonts = setup.definitions();
    if matches!(TYPEFACE, Typeface::Quattro) {
        for (name, bytes, weight) in [
            ("quattro-regular", QUATTRO_REGULAR, Weight::Regular),
            ("quattro-bold", QUATTRO_BOLD, Weight::SemiBold),
        ] {
            fonts
                .font_data
                .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
            if let Some(family) = fonts.families.get_mut(&weight.family()) {
                family.insert(0, name.into());
            }
        }
    }
    let rendering = fastframe_text::detect();
    rendering.apply_to(&mut fonts);
    ctx.set_fonts(fonts);

    fastframe_emoji::EmojiSetup::default().install();
    std::thread::spawn(fastframe_emoji::warm_up);
    ctx.add_plugin(fastframe_emoji::EmojiPlugin::default());

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(SMALL, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(BODY, FontFamily::Proportional),
            ),
            (TextStyle::Heading, font(LARGE, Weight::SemiBold)),
            (
                TextStyle::Monospace,
                FontId::new(SMALL, FontFamily::Monospace),
            ),
        ]
        .into();
        style.interaction.selectable_labels = false;
        // Views place things on the grid themselves; egui's gaps between widgets would push them off it.
        style.spacing.item_spacing = egui::vec2(2.0 * UNIT, 0.0);
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        rendering.apply_to_visuals(v);
        v.panel_fill = BG;
        v.window_fill = BG;
        v.extreme_bg_color = BG;
        v.faint_bg_color = HOVER;
        v.override_text_color = None;
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = SELECTED;
        v.selection.stroke = Stroke::new(1.0, TEXT);
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        for (state, fill) in [
            (&mut w.inactive, BG),
            (&mut w.hovered, HOVER),
            (&mut w.active, SELECTED),
            (&mut w.open, SELECTED),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0, TEXT);
            state.expansion = 0.0;
        }
        v.weak_text_color = Some(SECONDARY);
    });
}
