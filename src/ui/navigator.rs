//! The navigator, as image viewers have it: the picture in small with the
//! part on screen framed. The frame is dragged to move the pan, a press
//! elsewhere puts the frame's centre there first, a double click centres
//! the picture. On the panel's Video tab, and over the picture's corner
//! while it is zoomed in past its area.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use super::style;
use crate::adjust::{Adjust, PAN_MAX};

/// What the picture is shown in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// The picture's shape as shown (turned a quarter: the other way),
    /// width over height.
    pub aspect: f32,
    /// The area it is shown in, in points.
    pub area: Vec2,
}

impl Geometry {
    /// For a picture of `size` (mpv's `dwidth`, `dheight`, turned) in
    /// `area`; a 16:9 one while its size is not known.
    pub fn new(size: Option<[f64; 2]>, area: Vec2) -> Self {
        let aspect = size
            .filter(|[w, h]| *w > 0.0 && *h > 0.0)
            .map_or(16.0 / 9.0, |[w, h]| (w / h) as f32);
        Self { aspect, area }
    }

    /// The part of the picture on screen, in fractions of the picture
    /// (0 to 1 across and down; past them where the area shows more):
    /// mpv fits the picture in the area, zooms it and moves it by the pan
    /// in fractions of its zoomed size (`video-pan-x`).
    pub fn view(self, a: &Adjust) -> Rect {
        let centre = pos2(0.5 - a.pan_x as f32 / 100.0, 0.5 - a.pan_y as f32 / 100.0);
        Rect::from_center_size(centre, self.view_size(a))
    }

    /// The size of [`Self::view`], whatever the pan.
    fn view_size(self, a: &Adjust) -> Vec2 {
        if self.aspect <= 0.0 || self.area.x <= 0.0 || self.area.y <= 0.0 {
            return Vec2::splat(1.0);
        }
        // The picture's height when it fits, then zoomed.
        let height = (self.area.x / self.aspect).min(self.area.y) * a.zoom as f32 / 100.0;
        vec2(self.area.x / (height * self.aspect), self.area.y / height)
    }

    /// The picture is larger than its area across or down: the corner's
    /// navigator is shown.
    pub fn zoomed_in(self, a: &Adjust) -> bool {
        let size = self.view_size(a);
        size.x < 0.999 || size.y < 0.999
    }

    /// What the navigator covers, in fractions of the picture: the picture
    /// and the view as it is centred (the area beyond a picture zoomed
    /// out). Not the pan: the drawing would move under the pointer.
    fn bounds(self, a: &Adjust) -> Rect {
        Rect::from_center_size(pos2(0.5, 0.5), self.view_size(a).max(Vec2::splat(1.0)))
    }

    /// The navigator's size within `max`, its shape that of [`Self::bounds`].
    pub fn size(self, a: &Adjust, max: Vec2) -> Vec2 {
        let b = self.bounds(a).size();
        let shape = b.x * self.aspect / b.y;
        let width = max.x.min(max.y * shape);
        vec2(width, width / shape)
    }
}

/// The navigator in `rect` ([`Geometry::size`] gives its size) on `ground`:
/// moves `a`'s pan as the pointer says; true when it did.
pub fn show(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    geometry: Geometry,
    a: &mut Adjust,
    ground: Color32,
) -> bool {
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let bounds = geometry.bounds(a);
    // Fractions of the picture to points and back.
    let scale = vec2(
        rect.width() / bounds.width(),
        rect.height() / bounds.height(),
    );
    let to_screen = |p: Pos2| rect.min + (p - bounds.min) * scale;
    let to_picture = |p: Pos2| bounds.min + (p - rect.min) / scale;
    let before = (a.pan_x, a.pan_y);
    let set_centre = |a: &mut Adjust, centre: Pos2| {
        a.pan_x = ((0.5 - centre.x) as f64 * 100.0).clamp(-PAN_MAX, PAN_MAX);
        a.pan_y = ((0.5 - centre.y) as f64 * 100.0).clamp(-PAN_MAX, PAN_MAX);
    };
    if response.double_clicked() {
        (a.pan_x, a.pan_y) = (0.0, 0.0);
    } else if response.is_pointer_button_down_on() {
        let (pressed, at, delta) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.interact_pos(),
                i.pointer.delta(),
            )
        });
        let view = geometry.view(a);
        match at {
            // A press off the frame puts its centre there; then it follows
            // the pointer (its own delta: egui's drag starts points later).
            Some(at) if pressed && !view.contains(to_picture(at)) => set_centre(a, to_picture(at)),
            Some(_) if !pressed => set_centre(a, view.center() + delta / scale),
            _ => {}
        }
    }
    let dragging = response.is_pointer_button_down_on();
    let cursor = if dragging {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    };
    let response = response.on_hover_cursor(cursor);

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ground);
    let picture = Rect::from_min_max(to_screen(Pos2::ZERO), to_screen(pos2(1.0, 1.0)));
    painter.rect_filled(picture, 0.0, Color32::from_white_alpha(18));
    painter.rect_stroke(
        picture,
        0.0,
        Stroke::new(1.0, Color32::from_white_alpha(60)),
        egui::StrokeKind::Inside,
    );
    let view = geometry.view(a);
    let frame = Rect::from_min_max(to_screen(view.min), to_screen(view.max));
    let accent = if response.hovered() || dragging {
        style::accent_hover()
    } else {
        style::accent()
    };
    painter.rect_filled(frame, 0.0, accent.gamma_multiply(0.18));
    painter.rect_stroke(
        frame,
        0.0,
        Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );
    (a.pan_x, a.pan_y) != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Rect, b: Rect) -> bool {
        (a.min - b.min).length() < 1e-4 && (a.max - b.max).length() < 1e-4
    }

    #[test]
    fn view_follows_zoom_and_pan() {
        // A 16:9 picture in a 16:9 area.
        let g = Geometry::new(Some([1920.0, 1080.0]), vec2(1600.0, 900.0));
        let mut a = Adjust::default();
        assert!(close(
            g.view(&a),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
        ));
        assert!(!g.zoomed_in(&a));
        a.zoom = 200;
        assert!(close(
            g.view(&a),
            Rect::from_min_max(pos2(0.25, 0.25), pos2(0.75, 0.75))
        ));
        assert!(g.zoomed_in(&a));
        // The picture moved right and down: its left and top part shown.
        a.pan_x = 25.0;
        a.pan_y = 10.0;
        assert!(close(
            g.view(&a),
            Rect::from_min_max(pos2(0.0, 0.15), pos2(0.5, 0.65))
        ));
        // A wide picture in a 4:3 area: black bars above and below.
        let wide = Geometry::new(Some([1920.0, 800.0]), vec2(800.0, 600.0));
        let v = wide.view(&Adjust::default());
        assert!((v.width() - 1.0).abs() < 1e-4 && v.height() > 1.0);
        // At 150 % wider than the area, still lower: zoomed in across only.
        let a = Adjust {
            zoom: 150,
            ..Adjust::default()
        };
        assert!(wide.zoomed_in(&a));
        let v = wide.view(&a);
        assert!(v.width() < 0.7 && v.height() > 1.1);
    }

    #[test]
    fn size_keeps_the_shape() {
        let g = Geometry::new(Some([1920.0, 1080.0]), vec2(1600.0, 900.0));
        let a = Adjust {
            zoom: 200,
            ..Adjust::default()
        };
        let s = g.size(&a, vec2(160.0, 160.0));
        assert!((s.x - 160.0).abs() < 1e-3 && (s.y - 90.0).abs() < 1e-3);
        // Zoomed out, the area beyond the picture counts.
        let out = Adjust {
            zoom: 50,
            ..Adjust::default()
        };
        assert!(g.bounds(&out).width() > 1.9);
        assert!(Geometry::new(None, Vec2::ZERO).view(&a).is_finite());
    }
}
