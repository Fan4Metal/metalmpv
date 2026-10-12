//! The window has no frame of Windows: the title row is the app's own
//! (`ui/title.rs`). It is still resizable for Windows (`WS_SIZEBOX` and
//! `WS_MAXIMIZEBOX` give Aero Snap, Win+arrows and maximising to the work
//! area), but winit gives the whole window to the client area, so the edges
//! resize it through strips of the app's own. No window region (qrec's way
//! to round corners): DWM keeps its shadow, and the corners are rounded by
//! DWM itself on Windows 11 (`win::round_corners`).

use egui::{CursorIcon, ResizeDirection as D, Sense, Ui, vec2};

/// Strips along the edges and squares at the corners hand a drag to
/// Windows (`ViewportCommand::BeginResize`), as a sizing frame would.
/// Placed before the panels, so that their widgets near an edge still take
/// clicks first. Not while maximized or in fullscreen.
pub fn resize_edges(ui: &mut Ui) {
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 14.0;
    let r = ui.ctx().content_rect();
    let grips = [
        (
            D::NorthWest,
            egui::Rect::from_min_size(r.left_top(), vec2(CORNER, CORNER)),
            CursorIcon::ResizeNwSe,
        ),
        (
            D::NorthEast,
            egui::Rect::from_min_size(r.right_top() - vec2(CORNER, 0.0), vec2(CORNER, CORNER)),
            CursorIcon::ResizeNeSw,
        ),
        (
            D::SouthWest,
            egui::Rect::from_min_size(r.left_bottom() - vec2(0.0, CORNER), vec2(CORNER, CORNER)),
            CursorIcon::ResizeNeSw,
        ),
        (
            D::SouthEast,
            egui::Rect::from_min_size(
                r.right_bottom() - vec2(CORNER, CORNER),
                vec2(CORNER, CORNER),
            ),
            CursorIcon::ResizeNwSe,
        ),
        (
            D::North,
            egui::Rect::from_min_max(r.left_top(), egui::pos2(r.right(), r.top() + EDGE)),
            CursorIcon::ResizeVertical,
        ),
        (
            D::South,
            egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - EDGE), r.right_bottom()),
            CursorIcon::ResizeVertical,
        ),
        (
            D::West,
            egui::Rect::from_min_max(r.left_top(), egui::pos2(r.left() + EDGE, r.bottom())),
            CursorIcon::ResizeHorizontal,
        ),
        (
            D::East,
            egui::Rect::from_min_max(egui::pos2(r.right() - EDGE, r.top()), r.right_bottom()),
            CursorIcon::ResizeHorizontal,
        ),
    ];
    // Corners first: where a corner and an edge overlap, the corner wins.
    let pointer = ui.ctx().pointer_hover_pos();
    let Some((direction, rect, icon)) = grips
        .into_iter()
        .find(|(_, rect, _)| pointer.is_some_and(|p| rect.contains(p)))
    else {
        return;
    };
    let response = ui.interact(
        rect,
        ui.id().with(("resize", direction as u8)),
        Sense::drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(icon);
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

/// Enter or leave fullscreen. winit moves the client area of an undecorated
/// window with a shadow a pixel down (its `WM_NCCALCSIZE`), which in
/// fullscreen would push the bottom row of pixels off the screen: the
/// shadow is off while in fullscreen.
pub fn set_fullscreen(ctx: &egui::Context, frame: &eframe::Frame, on: bool) {
    if let Some(window) = frame.winit_window() {
        use winit::platform::windows::WindowExtWindows;
        window.set_undecorated_shadow(!on);
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
}

pub fn is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

pub fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}
