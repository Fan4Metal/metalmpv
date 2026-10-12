//! The look: always dark (a player shows pictures on black), one palette
//! for every colour of the interface.

use egui::Color32;

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
/// The Play button, the elapsed part of the seek bar, the volume.
pub const ACCENT: Color32 = rgb(0x2f, 0x7c, 0xf0);
pub const ACCENT_HOVER: Color32 = rgb(0x4a, 0x92, 0xff);
/// The rest of the seek bar.
pub const TRACK: Color32 = rgb(0x3a, 0x3e, 0x46);
/// The OSD text.
pub const OSD: Color32 = rgb(0xf3, 0xc5, 0x7a);
/// The playing entry of the playlist.
pub const CURRENT: Color32 = rgb(0x78, 0xb0, 0xff);
pub const SELECTED_BG: Color32 = rgb(0x26, 0x3b, 0x5e);
/// The close button of the title row under the pointer.
pub const CLOSE_HOVER: Color32 = rgb(0xc4, 0x2b, 0x1c);
pub const ERROR: Color32 = rgb(0xff, 0x8a, 0x80);

/// Height of the title row in points.
pub const TITLE_HEIGHT: f32 = 30.0;
/// Height of the row with the buttons.
pub const BUTTONS_HEIGHT: f32 = 46.0;
/// Height of the seek bar's row.
pub const SEEK_HEIGHT: f32 = 16.0;

/// The dark theme with this palette, for every widget of egui.
pub fn apply(ctx: &egui::Context) {
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
    ctx.set_fonts(fonts);
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
        v.selection.bg_fill = SELECTED_BG;
        v.selection.stroke.color = TEXT_BRIGHT;
        v.hyperlink_color = CURRENT;
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
    });
}
