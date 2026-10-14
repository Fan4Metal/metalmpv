//! What the windows over the control bar's buttons share: the Video, sound
//! and subtitles panel (`picture.rs`) and the tracks window (`tracks.rs`).
//! Each is an `egui::Area` of the `Foreground` order with a popup frame
//! over its button, not an egui `Popup`: those take the keys, and playback
//! and the keys go on here. A press outside it and its button closes it,
//! unless a popup of its own is open (a list, the colour picker, a menu):
//! the press is that one's.

use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

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

/// Tabs as one control across `width`: equal parts over a line, the one
/// `current` bright and underlined in the accent colour; `id` tells the
/// windows' tabs apart.
pub fn tabs(ui: &mut Ui, labels: &[&str], current: &mut usize, width: f32, id: &str) {
    let height = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter().clone();
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, style::LINE),
    );
    let part = width / labels.len().max(1) as f32;
    for (i, label) in labels.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + part * i as f32, rect.top()),
            vec2(part, height),
        );
        let response = ui.interact(r, ui.id().with((id, i)), Sense::click());
        if response.clicked() {
            *current = i;
        }
        let on = *current == i;
        if response.hovered() && !on {
            painter.rect_filled(r.shrink2(vec2(2.0, 3.0)), 4.0, style::HOVER);
        }
        let color = if on {
            style::TEXT_BRIGHT
        } else if response.hovered() {
            style::TEXT
        } else {
            style::TEXT_WEAK
        };
        painter.text(
            r.center() - vec2(0.0, 1.0),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(14.0),
            color,
        );
        if on {
            underline(&painter, r);
        }
    }
}

/// The accent line under a tab shown (or a title drawn as one).
pub fn underline(painter: &egui::Painter, r: Rect) {
    let half = r.width() * 0.3;
    let y = r.bottom() - 1.0;
    painter.line_segment(
        [pos2(r.center().x - half, y), pos2(r.center().x + half, y)],
        Stroke::new(2.0, style::accent()),
    );
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
    // An Area's Ui is as tall as it was last frame, so a list that scrolls
    // could shrink with the player but never grow back: all the room over
    // the corner is offered instead.
    let room = corner.y - ctx.content_rect().top() - EDGE - frame.total_margin().sum().y;
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(at)
        .constrain_to(ctx.content_rect())
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                ui.set_max_height(room.max(0.0));
                add(ui);
            });
        })
        .response
        .rect
}
