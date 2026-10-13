//! The icons of the buttons, drawn with lines and polygons: the default
//! fonts have no media symbols. Each takes the centre and a size `s` (the
//! icon's half width in points) and paints in `c`.

use egui::{Color32, Painter, Pos2, Shape, Stroke, pos2, vec2};

fn tri(painter: &Painter, points: [Pos2; 3], c: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), c, Stroke::NONE));
}

pub fn play(p: &Painter, o: Pos2, s: f32, c: Color32) {
    // Shifted right: the triangle's centre of mass sits left of its box's.
    let o = o + vec2(s * 0.15, 0.0);
    tri(
        p,
        [
            o + vec2(-s * 0.7, -s),
            o + vec2(s * 0.9, 0.0),
            o + vec2(-s * 0.7, s),
        ],
        c,
    );
}

pub fn pause(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let w = s * 0.42;
    for x in [-s * 0.6, s * 0.6 - w] {
        p.rect_filled(
            egui::Rect::from_min_size(o + vec2(x, -s * 0.9), vec2(w, s * 1.8)),
            1.0,
            c,
        );
    }
}

pub fn stop(p: &Painter, o: Pos2, s: f32, c: Color32) {
    p.rect_filled(
        egui::Rect::from_center_size(o, vec2(s * 1.5, s * 1.5)),
        1.5,
        c,
    );
}

/// |◀◀ (`forward` = false) or ▶▶|.
pub fn skip(p: &Painter, o: Pos2, s: f32, c: Color32, forward: bool) {
    let d = if forward { 1.0 } else { -1.0 };
    let h = s * 0.75;
    for x0 in [-s * 0.9, -s * 0.05] {
        let x0 = d * x0;
        tri(
            p,
            [
                o + vec2(x0, -h),
                o + vec2(x0 + d * s * 0.85, 0.0),
                o + vec2(x0, h),
            ],
            c,
        );
    }
    let x = o.x + d * s * 0.95;
    p.line_segment(
        [pos2(x, o.y - h), pos2(x, o.y + h)],
        Stroke::new(s * 0.22, c),
    );
}

/// ⏏: open a file.
pub fn eject(p: &Painter, o: Pos2, s: f32, c: Color32) {
    tri(
        p,
        [
            o + vec2(-s * 0.9, s * 0.15),
            o + vec2(0.0, -s * 0.85),
            o + vec2(s * 0.9, s * 0.15),
        ],
        c,
    );
    p.rect_filled(
        egui::Rect::from_min_max(o + vec2(-s * 0.9, s * 0.45), o + vec2(s * 0.9, s * 0.85)),
        1.0,
        c,
    );
}

/// A gear: the settings.
pub fn gear(p: &Painter, o: Pos2, s: f32, c: Color32, fill: Color32) {
    let stroke = Stroke::new(s * 0.32, c);
    for i in 0..8 {
        let a = std::f32::consts::TAU * i as f32 / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([o + d * s * 0.5, o + d * s * 0.98], stroke);
    }
    p.circle_filled(o, s * 0.68, c);
    p.circle_filled(o, s * 0.3, fill);
}

/// ≡ with dots: the playlist.
pub fn list(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(s * 0.2, c);
    for y in [-0.65, 0.0, 0.65] {
        let y = o.y + y * s;
        p.circle_filled(pos2(o.x - s * 0.8, y), s * 0.13, c);
        p.line_segment([pos2(o.x - s * 0.45, y), pos2(o.x + s * 0.9, y)], stroke);
    }
}

/// Four corners: fullscreen (`leave` points them inwards).
pub fn fullscreen(p: &Painter, o: Pos2, s: f32, c: Color32, leave: bool) {
    let stroke = Stroke::new(s * 0.2, c);
    let k = s * 0.45;
    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = o + vec2(dx * s * 0.9, dy * s * 0.75);
        let (hx, hy) = if leave { (dx, dy) } else { (-dx, -dy) };
        let corner = if leave {
            corner - vec2(dx * k, dy * k)
        } else {
            corner
        };
        p.line_segment([corner, corner + vec2(hx * k, 0.0)], stroke);
        p.line_segment([corner, corner + vec2(0.0, hy * k)], stroke);
    }
}

/// A loudspeaker with waves for `level` (0 to 1), or crossed out.
pub fn speaker(p: &Painter, o: Pos2, s: f32, c: Color32, level: f64, muted: bool) {
    let o = o - vec2(s * 0.35, 0.0);
    p.add(Shape::convex_polygon(
        vec![
            o + vec2(-s * 0.65, -s * 0.3),
            o + vec2(-s * 0.25, -s * 0.3),
            o + vec2(s * 0.25, -s * 0.75),
            o + vec2(s * 0.25, s * 0.75),
            o + vec2(-s * 0.25, s * 0.3),
            o + vec2(-s * 0.65, s * 0.3),
        ],
        c,
        Stroke::NONE,
    ));
    let stroke = Stroke::new(s * 0.15, c);
    if muted {
        let x = o + vec2(s * 0.95, 0.0);
        let k = s * 0.32;
        p.line_segment([x + vec2(-k, -k), x + vec2(k, k)], stroke);
        p.line_segment([x + vec2(-k, k), x + vec2(k, -k)], stroke);
        return;
    }
    let waves = if level <= 0.0 {
        0
    } else if level < 0.5 {
        1
    } else {
        2
    };
    for i in 0..waves {
        let r = s * (0.55 + 0.4 * i as f32);
        let points: Vec<Pos2> = (0..=8)
            .map(|k| {
                let a = -0.8 + 1.6 * k as f32 / 8.0;
                o + vec2(s * 0.2 + r * a.cos(), r * a.sin())
            })
            .collect();
        p.add(Shape::line(points, stroke));
    }
}

pub fn minimize(p: &Painter, o: Pos2, s: f32, c: Color32) {
    p.line_segment([o + vec2(-s, 0.0), o + vec2(s, 0.0)], Stroke::new(1.0, c));
}

/// □, or two overlapping squares while maximized.
pub fn maximize(p: &Painter, o: Pos2, s: f32, c: Color32, restore: bool) {
    let stroke = Stroke::new(1.0, c);
    if restore {
        let back = egui::Rect::from_min_size(o + vec2(-s * 0.6, -s), vec2(s * 1.6, s * 1.6));
        p.line_segment([back.left_top(), back.right_top()], stroke);
        p.line_segment([back.right_top(), back.right_bottom()], stroke);
        let front = egui::Rect::from_min_size(o + vec2(-s, -s * 0.6), vec2(s * 1.6, s * 1.6));
        p.rect_stroke(front, 0.0, stroke, egui::StrokeKind::Inside);
    } else {
        p.rect_stroke(
            egui::Rect::from_center_size(o, vec2(s * 2.0, s * 2.0)),
            0.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    }
}

pub fn close(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.1, c);
    p.line_segment([o + vec2(-s, -s), o + vec2(s, s)], stroke);
    p.line_segment([o + vec2(-s, s), o + vec2(s, -s)], stroke);
}

/// An "i" in a circle: the file's information.
pub fn info(p: &Painter, o: Pos2, s: f32, c: Color32) {
    p.circle_stroke(o, s, Stroke::new(1.1, c));
    p.circle_filled(o + vec2(0.0, -s * 0.45), 0.95, c);
    p.line_segment(
        [o + vec2(0.0, -s * 0.1), o + vec2(0.0, s * 0.55)],
        Stroke::new(1.3, c),
    );
}

/// A speech bubble with two lines of text: audio and subtitles.
pub fn tracks(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(s * 0.2, c);
    let body = egui::Rect::from_center_size(o - vec2(0.0, s * 0.15), vec2(s * 2.0, s * 1.45));
    p.rect_stroke(body, s * 0.3, stroke, egui::StrokeKind::Middle);
    // The tail, under the bottom left.
    let base = body.left_bottom() + vec2(s * 0.45, 0.0);
    p.add(Shape::line(
        vec![
            base,
            base + vec2(-s * 0.15, s * 0.6),
            base + vec2(s * 0.45, 0.0),
        ],
        stroke,
    ));
    let line = Stroke::new(s * 0.17, c);
    for (y, w) in [(-0.3, 1.1), (0.15, 0.7)] {
        let y = body.center().y + y * s;
        let x = body.left() + s * 0.45;
        p.line_segment([pos2(x, y), pos2(x + w * s, y)], line);
    }
}

/// +: add.
pub fn plus(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.6, c);
    p.line_segment([o + vec2(-s, 0.0), o + vec2(s, 0.0)], stroke);
    p.line_segment([o + vec2(0.0, -s), o + vec2(0.0, s)], stroke);
}

/// −: remove.
pub fn minus(p: &Painter, o: Pos2, s: f32, c: Color32) {
    p.line_segment([o + vec2(-s, 0.0), o + vec2(s, 0.0)], Stroke::new(1.6, c));
}

/// Bars getting shorter with an arrow down: sort.
pub fn sort(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.4, c);
    let left = o.x - s;
    for (i, w) in [1.0, 0.7, 0.4].into_iter().enumerate() {
        let y = o.y - s * 0.8 + i as f32 * s * 0.8;
        p.line_segment([pos2(left, y), pos2(left + s * 1.1 * w, y)], stroke);
    }
    let x = o.x + s * 0.65;
    p.line_segment([pos2(x, o.y - s * 0.9), pos2(x, o.y + s * 0.9)], stroke);
    p.line_segment(
        [pos2(x - s * 0.4, o.y + s * 0.5), pos2(x, o.y + s * 0.9)],
        stroke,
    );
    p.line_segment(
        [pos2(x + s * 0.4, o.y + s * 0.5), pos2(x, o.y + s * 0.9)],
        stroke,
    );
}

/// A waste bin: clear. qview's Delete icon (its `ui/toolbar.rs`), drawn
/// there 15 points high and scaled here to `s` (half the height).
pub fn trash(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let k = s / 7.5;
    let at = |x: f32, y: f32| o + vec2(x, y) * k;
    let stroke = Stroke::new(1.8 * k, c);
    // The handle, the lid, the body narrowing downwards and two ribs.
    p.add(Shape::line(
        vec![at(-2.5, -5.5), at(-2.5, -7.5), at(2.5, -7.5), at(2.5, -5.5)],
        stroke,
    ));
    p.line_segment([at(-7.0, -5.5), at(7.0, -5.5)], stroke);
    p.add(Shape::line(
        vec![at(-5.0, -3.0), at(-4.0, 7.0), at(4.0, 7.0), at(5.0, -3.0)],
        stroke,
    ));
    let rib = Stroke::new(1.4 * k, c);
    p.line_segment([at(-1.5, -0.5), at(-1.3, 4.5)], rib);
    p.line_segment([at(1.5, -0.5), at(1.3, 4.5)], rib);
}

/// Two sheets, one over the other: copy.
pub fn copy(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.4, c);
    let front = egui::Rect::from_min_max(o + vec2(-s * 0.4, -s * 0.4), o + vec2(s, s));
    p.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Middle);
    // Of the sheet behind, only what the front one leaves.
    p.add(Shape::line(
        vec![
            o + vec2(-s * 0.4, s * 0.4),
            o + vec2(-s, s * 0.4),
            o + vec2(-s, -s),
            o + vec2(s * 0.4, -s),
            o + vec2(s * 0.4, -s * 0.4),
        ],
        stroke,
    ));
}

/// An arrow down into a tray: save.
pub fn save(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.5, c);
    let tip = o + vec2(0.0, s * 0.35);
    p.line_segment([o + vec2(0.0, -s), tip], stroke);
    p.add(Shape::line(
        vec![
            tip + vec2(-s * 0.5, -s * 0.5),
            tip,
            tip + vec2(s * 0.5, -s * 0.5),
        ],
        stroke,
    ));
    p.add(Shape::line(
        vec![
            o + vec2(-s, s * 0.15),
            o + vec2(-s, s),
            o + vec2(s, s),
            o + vec2(s, s * 0.15),
        ],
        stroke,
    ));
}

/// A loop with an arrow: repeat; with `one`, a 1 inside (the file).
pub fn repeat(p: &Painter, o: Pos2, s: f32, c: Color32, one: bool) {
    let stroke = Stroke::new(1.4, c);
    let at = |x: f32, y: f32| o + vec2(x, y) * s;
    // Round from the gap at the top left, clockwise, to the arrow's tip.
    p.add(Shape::line(
        vec![
            at(0.15, -0.6),
            at(0.75, -0.6),
            at(1.0, -0.35),
            at(1.0, 0.35),
            at(0.75, 0.6),
            at(-0.75, 0.6),
            at(-1.0, 0.35),
            at(-1.0, -0.35),
            at(-0.75, -0.6),
            at(-0.1, -0.6),
        ],
        stroke,
    ));
    p.add(Shape::line(
        vec![at(-0.4, -0.88), at(-0.1, -0.6), at(-0.4, -0.32)],
        stroke,
    ));
    if one {
        p.text(
            at(0.0, 0.08),
            egui::Align2::CENTER_CENTER,
            "1",
            egui::FontId::proportional(s * 1.3),
            c,
        );
    }
}

/// Two crossing arrows: shuffle.
pub fn shuffle(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.4, c);
    let at = |x: f32, y: f32| o + vec2(x, y) * s;
    for y in [-0.55, 0.55] {
        p.add(Shape::line(
            vec![at(-1.0, -y), at(-0.4, -y), at(0.35, y), at(0.95, y)],
            stroke,
        ));
        p.add(Shape::line(
            vec![at(0.65, y - 0.3), at(0.95, y), at(0.65, y + 0.3)],
            stroke,
        ));
    }
}

/// A round arrow turning back (↺): back to the default.
pub fn reset(p: &Painter, o: Pos2, s: f32, c: Color32) {
    let stroke = Stroke::new(1.4, c);
    let r = 0.8 * s;
    // From the top round the left and the bottom to the right, where the
    // arrow points up: three quarters of a turn, against the clock.
    let points: Vec<Pos2> = (0..=18)
        .map(|i| {
            let a = (270.0 - 15.0 * i as f32).to_radians();
            o + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    let tip = o + vec2(r, -0.1 * s);
    p.add(Shape::line(points, stroke));
    p.add(Shape::line(
        vec![tip + vec2(-0.4, 0.4) * s, tip, tip + vec2(0.4, 0.4) * s],
        stroke,
    ));
}

/// A tick: done.
pub fn check(p: &Painter, o: Pos2, s: f32, c: Color32) {
    p.add(Shape::line(
        vec![
            o + vec2(-s * 0.85, 0.0),
            o + vec2(-s * 0.25, s * 0.6),
            o + vec2(s * 0.9, -s * 0.65),
        ],
        Stroke::new(1.8, c),
    ));
}
