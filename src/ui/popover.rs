//! What the windows over the control bar's buttons share: the Video, sound
//! and subtitles panel (`picture.rs`) and the tracks window (`tracks.rs`).
//! Each is an `egui::Area` of the `Foreground` order with a popup frame
//! over its button, not an egui `Popup`: those take the keys, and playback
//! and the keys go on here. A press outside it and its button closes it,
//! unless a popup of its own is open (a list, the colour picker, a menu):
//! the press is that one's.

use egui::{Align2, Pos2, Rect, Ui, vec2};

use super::style;

/// The room between a window and its button.
const GAP: f32 = 6.0;
/// A window's corner from the player's bottom right one when there is no
/// button (only the picture shown).
const CORNER: f32 = 12.0;
/// The room kept between a window and the player's top edge.
pub const EDGE: f32 = 8.0;

/// The frame of such a window.
pub fn frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
        .fill(style::PANEL_BG)
        .inner_margin(12)
}

/// Where a window's bottom right corner goes: over its button, its right
/// edge on the button's; without one, the player's bottom right corner.
pub fn corner(ctx: &egui::Context, button: Option<Rect>) -> Pos2 {
    match button {
        Some(b) => b.right_top() - vec2(0.0, GAP),
        None => ctx.content_rect().right_bottom() - vec2(CORNER, CORNER),
    }
}

/// A press this frame outside `shown` (where the window was drawn last)
/// and its `button`, with no popup of its own open: the window closes.
pub fn pressed_outside(ctx: &egui::Context, shown: Option<Rect>, button: Option<Rect>) -> bool {
    let pressed = ctx.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.press_origin())
            .flatten()
    });
    pressed.is_some_and(|p| {
        !egui::Popup::is_any_open(ctx)
            && !shown.is_some_and(|r| r.contains(p))
            && !button.is_some_and(|b| b.contains(p))
    })
}

/// Show `add` in window `id`, its content `width` wide, its bottom right at
/// `corner`; where it was drawn. The left edge is placed from the width the
/// window is meant to have: placed by the last frame's size, content a
/// little wider moved it, and it shook as it was laid out again.
pub fn show(
    ctx: &egui::Context,
    id: &str,
    corner: Pos2,
    width: f32,
    add: impl FnOnce(&mut Ui),
) -> Rect {
    let frame = frame(ctx);
    let at = corner - vec2(width + frame.total_margin().sum().x, 0.0);
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(at)
        .constrain_to(ctx.content_rect())
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                add(ui);
            });
        })
        .response
        .rect
}
