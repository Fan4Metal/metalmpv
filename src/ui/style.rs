//! The look: always dark (a player shows pictures on black), one palette
//! for every colour of the interface.

use std::sync::atomic::{AtomicU32, Ordering};

use egui::Color32;

use crate::icon::Screen;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The title row.
pub const TITLE_BG: Color32 = rgb(0x1c, 0x1e, 0x22);
/// The control bar under the picture.
pub const BAR_BG: Color32 = rgb(0x17, 0x19, 0x1d);
/// The playlist and dialogs.
pub const PANEL_BG: Color32 = rgb(0x20, 0x22, 0x27);
pub const LIST_BG: Color32 = rgb(0x15, 0x16, 0x19);
/// The category list of the settings.
pub const SIDEBAR_BG: Color32 = rgb(0x1a, 0x1c, 0x20);
/// A group of settings on a page.
pub const CARD_BG: Color32 = rgb(0x29, 0x2c, 0x32);
/// A key's name in the list of keys.
pub const KEY_BG: Color32 = rgb(0x38, 0x3c, 0x45);
/// Lines between parts.
pub const LINE: Color32 = rgb(0x2e, 0x31, 0x37);
pub const TEXT: Color32 = rgb(0xd4, 0xd7, 0xdc);
pub const TEXT_WEAK: Color32 = rgb(0x8a, 0x8f, 0x98);
pub const TEXT_BRIGHT: Color32 = rgb(0xff, 0xff, 0xff);
/// A hovered flat button's background.
pub const HOVER: Color32 = rgb(0x2d, 0x30, 0x37);
pub const PRESSED: Color32 = rgb(0x38, 0x3c, 0x44);
/// The rest of the seek bar.
pub const TRACK: Color32 = rgb(0x3a, 0x3e, 0x46);
/// The OSD text.
pub const OSD: Color32 = rgb(0xf3, 0xc5, 0x7a);
/// The close button of the title row under the pointer.
pub const CLOSE_HOVER: Color32 = rgb(0xc4, 0x2b, 0x1c);
pub const ERROR: Color32 = rgb(0xff, 0x8a, 0x80);

/// The accent colour chosen in the settings: the Play button, the seek
/// bar, the switches, the playing entry, the selection and the app icon's
/// screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Accent {
    #[default]
    Blue,
    Teal,
    Green,
    Orange,
    Red,
    Pink,
    Purple,
    Graphite,
    /// One of the user's own, `0xRRGGBB`.
    Custom(u32),
}

/// The colours that follow the accent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The Play button, the elapsed part of the seek bar, the volume.
    pub accent: Color32,
    pub hover: Color32,
    /// The playing entry of the playlist, links.
    pub current: Color32,
    /// The selected entry's ground.
    pub selected_bg: Color32,
    /// The app icon's screen.
    pub icon: Screen,
}

impl Accent {
    /// The ready ones.
    pub const ALL: [Accent; 8] = [
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Orange,
        Accent::Red,
        Accent::Pink,
        Accent::Purple,
        Accent::Graphite,
    ];

    /// The name in the settings: a ready one's, or `#RRGGBB`.
    pub fn name(self) -> String {
        let name = match self {
            Accent::Blue => "blue",
            Accent::Teal => "teal",
            Accent::Green => "green",
            Accent::Orange => "orange",
            Accent::Red => "red",
            Accent::Pink => "pink",
            Accent::Purple => "purple",
            Accent::Graphite => "graphite",
            Accent::Custom(c) => return format!("#{c:06X}"),
        };
        name.to_owned()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        if let Some([r, g, b]) = crate::sub_style::parse_hex(name) {
            return Some(Accent::Custom(u32::from_be_bytes([0, r, g, b])));
        }
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Accent::Blue => tr!("Blue", "Синий"),
            Accent::Teal => tr!("Teal", "Бирюзовый"),
            Accent::Green => tr!("Green", "Зелёный"),
            Accent::Orange => tr!("Orange", "Оранжевый"),
            Accent::Red => tr!("Red", "Красный"),
            Accent::Pink => tr!("Pink", "Розовый"),
            Accent::Purple => tr!("Purple", "Фиолетовый"),
            Accent::Graphite => tr!("Graphite", "Графитовый"),
            Accent::Custom(_) => tr!("Your own colour", "Свой цвет"),
        }
    }

    /// The colour itself, `0xRRGGBB`: the ready ones dark enough for white
    /// on them, light enough to stand out on the dark panels.
    pub fn base(self) -> u32 {
        match self {
            Accent::Blue => 0x2f7cf0,
            Accent::Teal => 0x0d9488,
            Accent::Green => 0x16a34a,
            Accent::Orange => 0xea580c,
            Accent::Red => 0xdc2626,
            Accent::Pink => 0xdb2777,
            Accent::Purple => 0x7c3aed,
            Accent::Graphite => 0x6b7280,
            Accent::Custom(c) => c & 0xffffff,
        }
    }

    /// Blue as it always was; the others' shades worked out from their
    /// colour as blue's nearly are.
    pub fn palette(self) -> Palette {
        if self == Accent::Blue {
            return Palette {
                accent: rgb(0x2f, 0x7c, 0xf0),
                hover: rgb(0x4a, 0x92, 0xff),
                current: rgb(0x78, 0xb0, 0xff),
                selected_bg: rgb(0x26, 0x3b, 0x5e),
                icon: Screen::BLUE,
            };
        }
        let base = self.base();
        let white = 0xffffff;
        let black = 0x000000;
        let panel = 0x202227;
        let color = |c: u32| rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
        Palette {
            accent: color(base),
            hover: color(mix(base, white, 0.14)),
            current: color(mix(base, white, 0.42)),
            selected_bg: color(mix(panel, base, 0.28)),
            icon: Screen {
                light: mix(base, white, 0.2),
                dark: mix(base, black, 0.15),
                shade: mix(base, black, 0.7),
            },
        }
    }
}

/// `a` moved towards `b` by `t`, both `0xRRGGBB`.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// The accent in use: the index of a ready one, or [`CUSTOM`] with the
/// colour in the low bits.
static ACCENT: AtomicU32 = AtomicU32::new(0);
const CUSTOM: u32 = 1 << 31;

/// Make `accent` the one the interface is drawn in ([`apply`] for egui's
/// own widgets).
pub fn set_accent(accent: Accent) {
    let value = match accent {
        Accent::Custom(c) => CUSTOM | (c & 0xffffff),
        _ => Accent::ALL.iter().position(|a| *a == accent).unwrap_or(0) as u32,
    };
    ACCENT.store(value, Ordering::Relaxed);
}

pub fn accent_choice() -> Accent {
    let value = ACCENT.load(Ordering::Relaxed);
    if value & CUSTOM != 0 {
        Accent::Custom(value & 0xffffff)
    } else {
        Accent::ALL[value as usize % Accent::ALL.len()]
    }
}

fn palette() -> Palette {
    accent_choice().palette()
}

/// The Play button, the elapsed part of the seek bar, the volume.
pub fn accent() -> Color32 {
    palette().accent
}

pub fn accent_hover() -> Color32 {
    palette().hover
}

/// The playing entry of the playlist.
pub fn current() -> Color32 {
    palette().current
}

pub fn selected_bg() -> Color32 {
    palette().selected_bg
}

/// Height of the title row in points.
pub const TITLE_HEIGHT: f32 = 30.0;
/// Height of the row with the buttons.
pub const BUTTONS_HEIGHT: f32 = 46.0;
/// Height of the seek bar's row.
pub const SEEK_HEIGHT: f32 = 16.0;

/// The font family of the OSD line: the font chosen for it, backed up by
/// the interface's (`set_fonts`).
pub fn osd_family() -> egui::FontFamily {
    egui::FontFamily::Name("osd".into())
}

/// The interface's fonts, and `osd` (a font chosen for the OSD line) in
/// front of them for [`osd_family`].
pub fn set_fonts(ctx: &egui::Context, osd: Option<egui::FontData>) {
    // The proportional font has no arrows (← → ↑ ↓ in the key names); the
    // monospace one, loaded anyway, has them: it backs the proportional
    // one up, as in qview.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(mono) = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .and_then(|f| f.first())
        .cloned()
        && let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional)
    {
        proportional.push(mono);
    }
    let mut osd_list = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(data) = osd {
        fonts
            .font_data
            .insert("osd".into(), std::sync::Arc::new(data));
        osd_list.insert(0, "osd".into());
    }
    fonts.families.insert(osd_family(), osd_list);
    ctx.set_fonts(fonts);
}

/// The dark theme with this palette, for every widget of egui.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        let v = &mut s.visuals;
        v.panel_fill = PANEL_BG;
        v.window_fill = PANEL_BG;
        v.extreme_bg_color = LIST_BG;
        v.window_stroke = egui::Stroke::new(1.0, LINE);
        v.widgets.noninteractive.fg_stroke.color = TEXT;
        v.widgets.noninteractive.bg_stroke.color = LINE;
        v.widgets.inactive.fg_stroke.color = TEXT;
        v.widgets.hovered.weak_bg_fill = HOVER;
        v.widgets.active.weak_bg_fill = PRESSED;
        v.selection.bg_fill = selected_bg();
        v.selection.stroke.color = TEXT_BRIGHT;
        v.hyperlink_color = current();
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_names() {
        for a in Accent::ALL {
            assert_eq!(Accent::from_name(&a.name()), Some(a));
        }
        assert_eq!(Accent::from_name("cyan"), None);
        let own = Accent::Custom(0x12ab9f);
        assert_eq!(own.name(), "#12AB9F");
        assert_eq!(Accent::from_name("#12AB9F"), Some(own));
        assert_eq!(own.palette().accent, rgb(0x12, 0xab, 0x9f));
        set_accent(own);
        assert_eq!(accent_choice(), own);
        set_accent(Accent::Purple);
        assert_eq!(accent_choice(), Accent::Purple);
        set_accent(Accent::Blue);
    }

    #[test]
    fn derived_shades() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert_eq!(mix(0x2f7cf0, 0x2f7cf0, 0.3), 0x2f7cf0);
        let red = Accent::Red.palette();
        assert_eq!(red.accent, rgb(0xdc, 0x26, 0x26));
        // Lighter for the hover and the playing entry, nearer the panel
        // for the selection.
        assert!(red.hover.g() > red.accent.g());
        assert!(red.current.g() > red.hover.g());
        assert!(red.selected_bg.r() < red.accent.r());
    }
}
