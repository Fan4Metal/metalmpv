//! The look: a theme (`Theme`: the grounds, lines and text) and an accent
//! colour. Every theme is dark: a player shows pictures on black, and a
//! light frame round them dazzles.

use std::sync::atomic::{AtomicU32, Ordering};

use egui::Color32;

use crate::icon::Screen;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const fn hex(c: u32) -> Color32 {
    rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// The text over the accent and the hovered or pressed controls; white
/// in every theme.
pub const TEXT_BRIGHT: Color32 = rgb(0xff, 0xff, 0xff);
/// The OSD text.
pub const OSD: Color32 = rgb(0xf3, 0xc5, 0x7a);
/// The A-B repeat's points on the seek bar: the OSD's yellow, which says
/// the same on the screen.
pub const LOOP: Color32 = OSD;
/// The close button of the title row under the pointer.
pub const CLOSE_HOVER: Color32 = rgb(0xc4, 0x2b, 0x1c);
pub const ERROR: Color32 = rgb(0xff, 0x8a, 0x80);

/// The base colours, chosen in the settings: from the darkest to the
/// lightest (a test holds the order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Black round the picture, as a cinema's.
    Black,
    /// Deep blue greys.
    Midnight,
    /// Dark greys with a touch of violet.
    Plum,
    /// Deep green greys.
    Forest,
    /// Warm dark browns.
    Espresso,
    /// Neutral dark greys: the player's first look.
    #[default]
    Graphite,
    /// Softer, lighter cool greys.
    Dimmed,
}

/// A theme's grounds, lines and text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grounds {
    /// The title row.
    pub title_bg: Color32,
    /// The control bar under the picture.
    pub bar_bg: Color32,
    /// The playlist and dialogs.
    pub panel_bg: Color32,
    pub list_bg: Color32,
    /// The category list of the settings.
    pub sidebar_bg: Color32,
    /// A group of settings on a page.
    pub card_bg: Color32,
    /// A key's name in the list of keys, and under the pointer.
    pub key_bg: Color32,
    pub key_hover: Color32,
    /// Lines between parts.
    pub line: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    /// A hovered flat button's background.
    pub hover: Color32,
    pub pressed: Color32,
    /// The rest of the seek bar.
    pub track: Color32,
    /// egui's widgets: a button's or a field's ground, under the pointer
    /// and pressed; the ground of what takes no input.
    pub widget: Color32,
    pub widget_hover: Color32,
    pub widget_active: Color32,
    pub sunk: Color32,
}

const GRAPHITE: Grounds = Grounds {
    title_bg: hex(0x1c1e22),
    bar_bg: hex(0x17191d),
    panel_bg: hex(0x202227),
    list_bg: hex(0x151619),
    sidebar_bg: hex(0x1a1c20),
    card_bg: hex(0x292c32),
    key_bg: hex(0x383c45),
    key_hover: hex(0x484d58),
    line: hex(0x2e3137),
    text: hex(0xd4d7dc),
    text_weak: hex(0x8a8f98),
    hover: hex(0x2d3037),
    pressed: hex(0x383c44),
    track: hex(0x3a3e46),
    widget: hex(0x3c3c3c),
    widget_hover: hex(0x464646),
    widget_active: hex(0x373737),
    sunk: hex(0x1b1b1b),
};

const BLACK: Grounds = Grounds {
    title_bg: hex(0x0b0b0b),
    bar_bg: hex(0x070707),
    panel_bg: hex(0x111111),
    list_bg: hex(0x050505),
    sidebar_bg: hex(0x0b0b0b),
    card_bg: hex(0x1e1e1e),
    key_bg: hex(0x2c2c2c),
    key_hover: hex(0x3c3c3c),
    line: hex(0x222222),
    text: hex(0xd6d6d6),
    text_weak: hex(0x8c8c8c),
    hover: hex(0x1f1f1f),
    pressed: hex(0x2c2c2c),
    track: hex(0x333333),
    widget: hex(0x353535),
    widget_hover: hex(0x404040),
    widget_active: hex(0x303030),
    sunk: hex(0x141414),
};

const MIDNIGHT: Grounds = Grounds {
    title_bg: hex(0x141a24),
    bar_bg: hex(0x10151d),
    panel_bg: hex(0x19202a),
    list_bg: hex(0x0e1218),
    sidebar_bg: hex(0x121822),
    card_bg: hex(0x232b37),
    key_bg: hex(0x323d4d),
    key_hover: hex(0x3f4b5e),
    line: hex(0x29323f),
    text: hex(0xd5dbe5),
    text_weak: hex(0x8792a5),
    hover: hex(0x232c39),
    pressed: hex(0x2e3848),
    track: hex(0x354052),
    widget: hex(0x36404f),
    widget_hover: hex(0x414c5d),
    widget_active: hex(0x313a48),
    sunk: hex(0x161c26),
};

const PLUM: Grounds = Grounds {
    title_bg: hex(0x1f1a25),
    bar_bg: hex(0x1a161f),
    panel_bg: hex(0x241e2b),
    list_bg: hex(0x17131b),
    sidebar_bg: hex(0x1c1722),
    card_bg: hex(0x2e2736),
    key_bg: hex(0x3e3548),
    key_hover: hex(0x4c425a),
    line: hex(0x342c3d),
    text: hex(0xdcd6e3),
    text_weak: hex(0x968da3),
    hover: hex(0x312939),
    pressed: hex(0x3d3447),
    track: hex(0x443b50),
    widget: hex(0x40384a),
    widget_hover: hex(0x4a4256),
    widget_active: hex(0x3a3344),
    sunk: hex(0x1d1824),
};

const FOREST: Grounds = Grounds {
    title_bg: hex(0x15201b),
    bar_bg: hex(0x111a16),
    panel_bg: hex(0x1a2520),
    list_bg: hex(0x0e1612),
    sidebar_bg: hex(0x131d18),
    card_bg: hex(0x23302a),
    key_bg: hex(0x314139),
    key_hover: hex(0x3d4f46),
    line: hex(0x2a3832),
    text: hex(0xd3e0d9),
    text_weak: hex(0x87998f),
    hover: hex(0x22302a),
    pressed: hex(0x2d3d35),
    track: hex(0x34463d),
    widget: hex(0x35453d),
    widget_hover: hex(0x405248),
    widget_active: hex(0x303f37),
    sunk: hex(0x161f1b),
};

const ESPRESSO: Grounds = Grounds {
    title_bg: hex(0x211c18),
    bar_bg: hex(0x1b1714),
    panel_bg: hex(0x26211d),
    list_bg: hex(0x181512),
    sidebar_bg: hex(0x1e1a16),
    card_bg: hex(0x302a25),
    key_bg: hex(0x403832),
    key_hover: hex(0x4e453e),
    line: hex(0x362f29),
    text: hex(0xdfd7cd),
    text_weak: hex(0x998e81),
    hover: hex(0x332c26),
    pressed: hex(0x3f3731),
    track: hex(0x463d36),
    widget: hex(0x433b34),
    widget_hover: hex(0x4d443c),
    widget_active: hex(0x3d3530),
    sunk: hex(0x1e1a16),
};

const DIMMED: Grounds = Grounds {
    title_bg: hex(0x2a2f36),
    bar_bg: hex(0x252a30),
    panel_bg: hex(0x30353d),
    list_bg: hex(0x23272e),
    sidebar_bg: hex(0x282d33),
    card_bg: hex(0x3a404a),
    key_bg: hex(0x4a515d),
    key_hover: hex(0x58606d),
    line: hex(0x424954),
    text: hex(0xdde1e6),
    text_weak: hex(0x9aa1ab),
    hover: hex(0x3b414a),
    pressed: hex(0x474e58),
    track: hex(0x4e5560),
    widget: hex(0x4b525d),
    widget_hover: hex(0x565d69),
    widget_active: hex(0x454b56),
    sunk: hex(0x2a2f36),
};

impl Theme {
    pub const ALL: [Theme; 7] = [
        Theme::Black,
        Theme::Midnight,
        Theme::Plum,
        Theme::Forest,
        Theme::Espresso,
        Theme::Graphite,
        Theme::Dimmed,
    ];

    /// The name in the settings.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Black => "black",
            Theme::Midnight => "midnight",
            Theme::Graphite => "graphite",
            Theme::Plum => "plum",
            Theme::Forest => "forest",
            Theme::Espresso => "espresso",
            Theme::Dimmed => "dimmed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Black => tr!("Black", "Чёрная"),
            Theme::Midnight => tr!("Midnight blue", "Тёмно-синяя"),
            Theme::Graphite => tr!("Graphite", "Графитовая"),
            Theme::Plum => tr!("Plum", "Сливовая"),
            Theme::Forest => tr!("Forest green", "Тёмно-зелёная"),
            Theme::Espresso => tr!("Espresso", "Кофейная"),
            Theme::Dimmed => tr!("Dimmed", "Приглушённая"),
        }
    }

    pub fn grounds(self) -> &'static Grounds {
        match self {
            Theme::Black => &BLACK,
            Theme::Midnight => &MIDNIGHT,
            Theme::Graphite => &GRAPHITE,
            Theme::Plum => &PLUM,
            Theme::Forest => &FOREST,
            Theme::Espresso => &ESPRESSO,
            Theme::Dimmed => &DIMMED,
        }
    }
}

/// The theme in use, its index in [`Theme::ALL`].
static THEME: AtomicU32 = AtomicU32::new(DEFAULT_THEME);
/// Graphite's index.
const DEFAULT_THEME: u32 = 5;

/// Make `theme` the one the interface is drawn in ([`apply`] for egui's
/// own widgets).
pub fn set_theme(theme: Theme) {
    let index = Theme::ALL
        .iter()
        .position(|t| *t == theme)
        .unwrap_or(DEFAULT_THEME as usize);
    THEME.store(index as u32, Ordering::Relaxed);
}

pub fn theme_choice() -> Theme {
    Theme::ALL
        .get(THEME.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

macro_rules! grounds {
    ($($name:ident),* $(,)?) => {
        $(
            #[inline]
            pub fn $name() -> Color32 {
                theme_choice().grounds().$name
            }
        )*
    };
}

grounds!(
    title_bg, bar_bg, panel_bg, list_bg, sidebar_bg, card_bg, key_bg, key_hover, line, text,
    text_weak, hover, pressed, track,
);

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

    /// A colour typed or pasted, in any of the forms of [`ColourText`]: six
    /// or three hex digits (`#abc` is `#aabbcc`), with `#` or without;
    /// `RGB(r, g, b)`, each 0–255, or the three numbers alone with commas;
    /// `HSL(h, s%, l%)`. Any case; spaces round them allowed.
    pub fn typed(text: &str) -> Option<Self> {
        let text = text.trim();
        let [r, g, b] = hex_typed(text)
            .or_else(|| rgb_typed(text))
            .or_else(|| hsl_typed(text))?;
        Some(Accent::Custom(u32::from_be_bytes([0, r, g, b])))
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

    /// The shades in the theme in use.
    pub fn palette(self) -> Palette {
        self.palette_in(theme_choice())
    }

    /// Blue as it always was; the others' shades worked out from their
    /// colour as blue's nearly are; the selection mixed with the theme's
    /// panel.
    pub fn palette_in(self, theme: Theme) -> Palette {
        let [r, g, b, _] = theme.grounds().panel_bg.to_array().map(u32::from);
        let panel = r << 16 | g << 8 | b;
        let base = self.base();
        let selected_bg = hex(mix(panel, base, 0.28));
        if self == Accent::Blue {
            return Palette {
                accent: rgb(0x2f, 0x7c, 0xf0),
                hover: rgb(0x4a, 0x92, 0xff),
                current: rgb(0x78, 0xb0, 0xff),
                selected_bg: if theme == Theme::Graphite {
                    rgb(0x26, 0x3b, 0x5e)
                } else {
                    selected_bg
                },
                icon: Screen::BLUE,
            };
        }
        let white = 0xffffff;
        let black = 0x000000;
        Palette {
            accent: hex(base),
            hover: hex(mix(base, white, 0.14)),
            current: hex(mix(base, white, 0.42)),
            selected_bg,
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
        // egui's own dark visuals under the theme's colours, so that a
        // theme switched to leaves nothing of the one before.
        s.visuals = egui::Visuals::dark();
        let g = theme_choice().grounds();
        let v = &mut s.visuals;
        v.panel_fill = g.panel_bg;
        v.window_fill = g.panel_bg;
        v.extreme_bg_color = g.list_bg;
        v.window_stroke = egui::Stroke::new(1.0, g.line);
        v.widgets.noninteractive.fg_stroke.color = g.text;
        v.widgets.noninteractive.bg_stroke.color = g.line;
        v.widgets.noninteractive.bg_fill = g.sunk;
        v.widgets.noninteractive.weak_bg_fill = g.sunk;
        v.widgets.inactive.fg_stroke.color = g.text;
        v.widgets.inactive.bg_fill = g.widget;
        v.widgets.inactive.weak_bg_fill = g.widget;
        v.widgets.hovered.bg_fill = g.widget_hover;
        v.widgets.hovered.weak_bg_fill = g.hover;
        v.widgets.active.bg_fill = g.widget_active;
        v.widgets.active.weak_bg_fill = g.pressed;
        v.selection.bg_fill = selected_bg();
        v.selection.stroke.color = TEXT_BRIGHT;
        v.hyperlink_color = current();
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
    });
}

/// How the own colour's code is shown: `#RRGGBB`, `RGB(r, g, b)` or
/// `HSL(h, s%, l%)`; any of them is read back (`Accent::typed`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColourText {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl ColourText {
    pub fn next(self) -> Self {
        match self {
            ColourText::Hex => ColourText::Rgb,
            ColourText::Rgb => ColourText::Hsl,
            ColourText::Hsl => ColourText::Hex,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ColourText::Hex => "HEX",
            ColourText::Rgb => "RGB",
            ColourText::Hsl => "HSL",
        }
    }

    pub fn show(self, [r, g, b]: [u8; 3]) -> String {
        match self {
            ColourText::Hex => format!("#{r:02X}{g:02X}{b:02X}"),
            ColourText::Rgb => format!("RGB({r}, {g}, {b})"),
            ColourText::Hsl => {
                let [h, s, l] = hsl_of([r, g, b]);
                format!(
                    "HSL({:.0}, {:.0}%, {:.0}%)",
                    h.round() % 360.0,
                    s * 100.0,
                    l * 100.0
                )
            }
        }
    }
}

/// Six or three hex digits, `#` before them or not.
fn hex_typed(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    let six = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    crate::sub_style::parse_hex(&format!("#{six}"))
}

/// The three numbers inside `name(…)` (any case), or the text itself
/// when it has no such name but commas; units after them are the caller's.
fn parts<'a>(text: &'a str, name: &str) -> Option<Vec<&'a str>> {
    let inner = match text.get(..name.len()) {
        Some(head) if head.eq_ignore_ascii_case(name) => text[name.len()..]
            .trim()
            .strip_prefix('(')?
            .strip_suffix(')')?,
        _ if text.contains(',') && !text.contains('(') => text,
        _ => return None,
    };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    (parts.len() == 3).then_some(parts)
}

/// `RGB(r, g, b)` or `r, g, b`, each 0–255.
fn rgb_typed(text: &str) -> Option<[u8; 3]> {
    let parts = parts(text, "rgb")?;
    let mut rgb = [0; 3];
    for (value, part) in rgb.iter_mut().zip(parts) {
        *value = part.parse().ok()?;
    }
    Some(rgb)
}

/// `HSL(h, s%, l%)`: the hue in degrees (`deg` or `°` after it allowed),
/// the others in per cent (`%` optional), 0–100.
fn hsl_typed(text: &str) -> Option<[u8; 3]> {
    let parts = parts(text, "hsl")?;
    let number = |part: &str, units: &[&str]| {
        let part = units
            .iter()
            .fold(part, |p, u| p.strip_suffix(u).unwrap_or(p))
            .trim();
        part.parse::<f32>().ok().filter(|v| v.is_finite())
    };
    let h = number(parts[0], &["deg", "°"])?;
    let s = number(parts[1], &["%"]).filter(|v| (0.0..=100.0).contains(v))?;
    let l = number(parts[2], &["%"]).filter(|v| (0.0..=100.0).contains(v))?;
    Some(rgb_of_hsl(h.rem_euclid(360.0), s / 100.0, l / 100.0))
}

/// Hue in degrees, saturation and lightness 0–1.
fn hsl_of([r, g, b]: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, s, l]
}

fn rgb_of_hsl(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_texts() {
        let teal = [0x44, 0x99, 0x99];
        assert_eq!(ColourText::Hex.show(teal), "#449999");
        assert_eq!(ColourText::Rgb.show(teal), "RGB(68, 153, 153)");
        assert_eq!(ColourText::Hsl.show(teal), "HSL(180, 38%, 43%)");
        for mode in [ColourText::Hex, ColourText::Rgb] {
            assert_eq!(
                Accent::typed(&mode.show(teal)),
                Some(Accent::Custom(0x449999))
            );
        }
        // HSL in whole numbers comes back within a step of each part.
        let Some(Accent::Custom(c)) = Accent::typed(&ColourText::Hsl.show(teal)) else {
            panic!("HSL not read");
        };
        let [_, r, g, b] = c.to_be_bytes();
        for (got, was) in [r, g, b].into_iter().zip(teal) {
            assert!(got.abs_diff(was) <= 2, "{got} for {was}");
        }
        assert_eq!(
            Accent::typed("rgb( 255 ,0, 10 )"),
            Some(Accent::Custom(0xff000a))
        );
        assert_eq!(Accent::typed("255, 0, 10"), Some(Accent::Custom(0xff000a)));
        assert_eq!(Accent::typed("RGB(256, 0, 0)"), None);
        assert_eq!(Accent::typed("RGB(1, 2)"), None);
        assert_eq!(
            Accent::typed("hsl(0, 100%, 50%)"),
            Some(Accent::Custom(0xff0000))
        );
        assert_eq!(
            Accent::typed("HSL(120deg, 100, 25)"),
            Some(Accent::Custom(0x008000))
        );
        assert_eq!(
            Accent::typed("hsl(240°, 100%, 50%)"),
            Some(Accent::Custom(0x0000ff))
        );
        assert_eq!(
            Accent::typed("hsl(0, 0%, 100%)"),
            Some(Accent::Custom(0xffffff))
        );
        assert_eq!(Accent::typed("hsl(0, 120%, 50%)"), None);
        assert_eq!(ColourText::Hsl.show([255, 255, 255]), "HSL(0, 0%, 100%)");
        assert_eq!(ColourText::Hsl.show([0, 0, 0]), "HSL(0, 0%, 0%)");
        assert_eq!(ColourText::Hex.next().next().next(), ColourText::Hex);
    }

    #[test]
    fn accent_names() {
        for a in Accent::ALL {
            assert_eq!(Accent::from_name(&a.name()), Some(a));
        }
        assert_eq!(Accent::from_name("cyan"), None);
        let own = Accent::Custom(0x12ab9f);
        assert_eq!(own.name(), "#12AB9F");
        assert_eq!(Accent::from_name("#12AB9F"), Some(own));
        assert_eq!(Accent::typed("12ab9f"), Some(own));
        assert_eq!(
            Accent::typed(
                " #12AB9F
"
            ),
            Some(own)
        );
        assert_eq!(Accent::typed("#1a9"), Some(Accent::Custom(0x11aa99)));
        assert_eq!(Accent::typed("1a9"), Some(Accent::Custom(0x11aa99)));
        assert_eq!(Accent::typed("#12ab9"), None);
        assert_eq!(Accent::typed("#ggg"), None);
        assert_eq!(Accent::typed("#ЖЖЖ"), None);
        assert_eq!(own.palette().accent, rgb(0x12, 0xab, 0x9f));
        set_accent(own);
        assert_eq!(accent_choice(), own);
        set_accent(Accent::Purple);
        assert_eq!(accent_choice(), Accent::Purple);
        set_accent(Accent::Blue);
    }

    fn luminance(c: Color32) -> f32 {
        let linear = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }

    /// WCAG's contrast ratio.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn theme_names() {
        for t in Theme::ALL {
            assert_eq!(Theme::from_name(t.name()), Some(t));
        }
        assert_eq!(Theme::from_name("light"), None);
        assert_eq!(Theme::default(), Theme::Graphite);
        assert_eq!(Theme::ALL[DEFAULT_THEME as usize], Theme::Graphite);
        set_theme(Theme::Espresso);
        assert_eq!(theme_choice(), Theme::Espresso);
        set_theme(Theme::default());
        assert_eq!(theme_choice(), Theme::Graphite);
    }

    #[test]
    fn the_default_is_the_first_look() {
        let g = Theme::Graphite.grounds();
        assert_eq!(g.panel_bg, rgb(0x20, 0x22, 0x27));
        assert_eq!(g.text, rgb(0xd4, 0xd7, 0xdc));
        // egui's own dark greys for its widgets.
        let dark = egui::Visuals::dark().widgets;
        assert_eq!(g.widget, dark.inactive.bg_fill);
        assert_eq!(g.widget_hover, dark.hovered.bg_fill);
        assert_eq!(g.widget_active, dark.active.bg_fill);
        assert_eq!(g.sunk, dark.noninteractive.bg_fill);
        assert_eq!(
            Accent::Blue.palette_in(Theme::Graphite).selected_bg,
            rgb(0x26, 0x3b, 0x5e)
        );
    }

    #[test]
    fn themes_go_from_the_darkest_to_the_lightest() {
        // The window's mean luminance about the picture: the bar, the
        // title row and the playlist.
        let light = |t: Theme| {
            let g = t.grounds();
            0.4 * luminance(g.bar_bg) + 0.2 * luminance(g.title_bg) + 0.4 * luminance(g.list_bg)
        };
        for pair in Theme::ALL.windows(2) {
            assert!(
                light(pair[0]) < light(pair[1]),
                "{:?} before {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn every_theme_can_be_read() {
        for theme in Theme::ALL {
            let g = theme.grounds();
            let what = format!("{theme:?}");
            for ground in [g.list_bg, g.panel_bg, g.bar_bg, g.title_bg] {
                assert!(contrast(g.text, ground) >= 7.0, "{what}: text");
                assert!(contrast(g.text_weak, ground) >= 4.0, "{what}: weak text");
            }
            assert!(contrast(g.text, g.card_bg) >= 7.0, "{what}: text on a card");
            assert!(
                contrast(g.text_weak, g.card_bg) >= 3.5,
                "{what}: weak on a card"
            );
            assert!(contrast(g.text, g.key_bg) >= 4.5, "{what}: a key");
            assert!(contrast(g.text, g.widget) >= 4.5, "{what}: a button");
            assert!(contrast(TEXT_BRIGHT, g.pressed) >= 7.0, "{what}: pressed");
            // The parts tell apart from one another.
            assert!(contrast(g.track, g.bar_bg) >= 1.4, "{what}: the seek bar");
            assert!(contrast(g.card_bg, g.panel_bg) >= 1.1, "{what}: cards");
            assert!(contrast(g.hover, g.bar_bg) >= 1.1, "{what}: hover");
            for accent in Accent::ALL {
                let p = accent.palette_in(theme);
                let what = format!("{what} {accent:?}");
                assert!(contrast(g.text, p.selected_bg) >= 4.5, "{what}: selected");
                assert!(contrast(p.current, g.list_bg) >= 4.5, "{what}: playing");
                assert!(contrast(p.accent, g.bar_bg) >= 2.0, "{what}: the accent");
            }
        }
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
