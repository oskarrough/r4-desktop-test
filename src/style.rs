//! The look: one grid, three type sizes, a dark and a light palette with one accent.
//! Every edge sits on a multiple of `UNIT`; text starts `MARGIN` in from the window's sides.
use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
use fastframe_fonts::{LocalFont, Weight};
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, OnceLock};

pub const UNIT: f32 = 4.0;
/// Left and right of everything: text, bars, the last button's glyph.
pub const MARGIN: f32 = 4.0 * UNIT;

/// Times, and the line under a title.
pub const SMALL: f32 = 11.0;
/// Rows, headers, the transport's title.
pub const BODY: f32 = 13.0;
/// The now-playing title.
pub const LARGE: f32 = 18.0;

/// Which typeface leads. Flip this to go back to Radio Canada.
const TYPEFACE: Typeface = Typeface::Quattro;

/// The leading face's capital height, as a share of the font size.
pub const CAP: f32 = TYPEFACE.cap();

/// The colours, from `assets/themes`: one palette for dark and one for light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub hairline: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub disabled: Color32,
    /// What floats: round buttons, the title pill and the mini player, and their edge and shadow.
    pub surface: Color32,
    pub edge: Color32,
    pub shadow: Color32,
    /// Only for what is live: progress, the playing dot, the channel link.
    pub accent: Color32,
}

const DARK: &str = include_str!("../assets/themes/dark.json");
const LIGHT: &str = include_str!("../assets/themes/light.json");

impl Palette {
    pub fn dark() -> Self {
        static OWN: LazyLock<Palette> = LazyLock::new(|| Palette::own(true, DARK));
        *OWN
    }

    pub fn light() -> Self {
        static OWN: LazyLock<Palette> = LazyLock::new(|| Palette::own(false, LIGHT));
        *OWN
    }

    /// One of ours: every colour named in its file, so nothing is left to a default.
    fn own(dark: bool, file: &str) -> Self {
        #[derive(serde::Deserialize)]
        struct File {
            colors: BTreeMap<String, String>,
        }
        let file: File = serde_json::from_str(file).expect("our palette is valid JSON");
        let mut palette = Self {
            dark,
            bg: Color32::PLACEHOLDER,
            hairline: Color32::PLACEHOLDER,
            hover: Color32::PLACEHOLDER,
            selected: Color32::PLACEHOLDER,
            text: Color32::PLACEHOLDER,
            secondary: Color32::PLACEHOLDER,
            disabled: Color32::PLACEHOLDER,
            surface: Color32::PLACEHOLDER,
            edge: Color32::PLACEHOLDER,
            shadow: Color32::PLACEHOLDER,
            accent: Color32::PLACEHOLDER,
        };
        for (name, value) in &file.colors {
            let color = fastframe_theme::parse_color(value).expect("our colours are #RRGGBB");
            assert!(palette.set(name, color), "unknown colour {name}");
        }
        palette
    }

    /// Sets a colour by its name in a palette file, named as fastframe-theme's.
    fn set(&mut self, name: &str, color: Color32) -> bool {
        match name {
            "window" => self.bg = color,
            "outline" => self.hairline = color,
            "surface_hover" => self.hover = color,
            "surface_active" => self.selected = color,
            "text" => self.text = color,
            "secondary" => self.secondary = color,
            "dim" => self.disabled = color,
            "surface" => self.surface = color,
            "edge" => self.edge = color,
            "shadow" => self.shadow = color,
            "accent" => self.accent = color,
            _ => return false,
        }
        true
    }
}

/// The palette the views draw with, as `apply` last set it.
pub fn palette(ui: &egui::Ui) -> Palette {
    ui.ctx()
        .data(|d| d.get_temp(egui::Id::NULL))
        .unwrap_or_else(Palette::dark)
}

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
    let rendering = RENDERING.get_or_init(fastframe_text::detect);
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
        // Lists draw their own header and dock fades.
        style.spacing.scroll.fade.strength = 0.0;
    });
    apply(ctx, Palette::dark());
}

/// The desktop's text rendering, asked once: dark and light text are drawn differently.
static RENDERING: OnceLock<fastframe_text::TextRendering> = OnceLock::new();

/// Paints egui's own widgets in `p`, and hands `p` to the views.
pub fn apply(ctx: &egui::Context, p: Palette) {
    let rendering = RENDERING.get_or_init(fastframe_text::detect);
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        *v = if p.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        rendering.apply_to_visuals(v);
        v.panel_fill = p.bg;
        v.window_fill = p.bg;
        v.extreme_bg_color = p.bg;
        v.faint_bg_color = p.hover;
        v.override_text_color = None;
        v.hyperlink_color = p.accent;
        v.selection.bg_fill = p.selected;
        v.selection.stroke = Stroke::new(1.0, p.text);
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.hairline);
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
        for (state, fill) in [
            (&mut w.inactive, p.bg),
            (&mut w.hovered, p.hover),
            (&mut w.active, p.selected),
            (&mut w.open, p.selected),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0, p.text);
            state.expansion = 0.0;
        }
        v.weak_text_color = Some(p.secondary);
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, p));
}
