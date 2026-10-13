//! The playlist in a window of its own beside the player's: drawn to the
//! player's edges while it is moved (a magnet, as in GOM Player), and
//! glued to the edge it touches, so that it goes along when the player is
//! moved or resized. Rectangles are the windows' visible ones in pixels
//! (left, top, right, bottom), without a framed window's invisible resize
//! borders.

pub type Rect = [i32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The player's edge the playlist touches, and where along it the
/// playlist starts (from the player's top, or its left for the top and
/// bottom edges).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glue {
    pub side: Side,
    pub offset: i32,
}

/// How far apart two edges may be and still touch.
const TOUCH: i32 = 1;

fn overlaps(a0: i32, a1: i32, b0: i32, b1: i32) -> bool {
    a0 < b1 && b0 < a1
}

/// The edge of `main` that `child` touches, if any.
pub fn glue_of(main: Rect, child: Rect) -> Option<Glue> {
    let [ml, mt, mr, mb] = main;
    let [cl, ct, cr, cb] = child;
    let across = overlaps(ct, cb, mt, mb);
    let along = overlaps(cl, cr, ml, mr);
    let side = if across && (cl - mr).abs() <= TOUCH {
        Side::Right
    } else if across && (cr - ml).abs() <= TOUCH {
        Side::Left
    } else if along && (ct - mb).abs() <= TOUCH {
        Side::Bottom
    } else if along && (cb - mt).abs() <= TOUCH {
        Side::Top
    } else {
        return None;
    };
    let offset = match side {
        Side::Left | Side::Right => ct - mt,
        Side::Top | Side::Bottom => cl - ml,
    };
    Some(Glue { side, offset })
}

/// Where the glued playlist of `size` goes beside `main`.
pub fn placed(main: Rect, size: [i32; 2], glue: Glue) -> Rect {
    let [ml, mt, mr, mb] = main;
    let [w, h] = size;
    let (l, t) = match glue.side {
        Side::Right => (mr, mt + glue.offset),
        Side::Left => (ml - w, mt + glue.offset),
        Side::Bottom => (ml + glue.offset, mb),
        Side::Top => (ml + glue.offset, mt - h),
    };
    [l, t, l + w, t + h]
}

/// `child`, being moved, drawn to the edges of `main` it comes within
/// `reach` of: beside it, then in line with its top or bottom (left or
/// right edge above and below it).
pub fn snapped(main: Rect, child: Rect, reach: i32) -> Rect {
    let [ml, mt, mr, mb] = main;
    let [cl, ct, cr, cb] = child;
    let (w, h) = (cr - cl, cb - ct);
    let near = |a: i32, b: i32| (a - b).abs() <= reach;
    let across = overlaps(ct - reach, cb + reach, mt, mb);
    let along = overlaps(cl - reach, cr + reach, ml, mr);
    let (mut l, mut t) = (cl, ct);
    if across && near(cl, mr) {
        l = mr;
    } else if across && near(cr, ml) {
        l = ml - w;
    } else if along && near(ct, mb) {
        t = mb;
    } else if along && near(cb, mt) {
        t = mt - h;
    }
    if l == mr || l + w == ml {
        if near(t, mt) {
            t = mt;
        } else if near(t + h, mb) {
            t = mb - h;
        }
    } else if t == mb || t + h == mt {
        if near(l, ml) {
            l = ml;
        } else if near(l + w, mr) {
            l = mr - w;
        }
    }
    [l, t, l + w, t + h]
}

/// The playlist beside a player at `main` the first time: on its right,
/// as tall as it, `width` wide.
pub fn first_place(main: Rect, width: i32) -> Rect {
    [main[2], main[1], main[2] + width, main[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = [100, 100, 900, 600];

    #[test]
    fn touching_edges_glue() {
        assert_eq!(
            glue_of(MAIN, [900, 150, 1200, 500]),
            Some(Glue {
                side: Side::Right,
                offset: 50
            })
        );
        assert_eq!(
            glue_of(MAIN, [-200, 100, 101, 600]),
            Some(Glue {
                side: Side::Left,
                offset: 0
            })
        );
        assert_eq!(
            glue_of(MAIN, [300, 600, 700, 900]),
            Some(Glue {
                side: Side::Bottom,
                offset: 200
            })
        );
        // Apart, or beside it but past its end.
        assert_eq!(glue_of(MAIN, [905, 150, 1200, 500]), None);
        assert_eq!(glue_of(MAIN, [900, 600, 1200, 800]), None);
    }

    #[test]
    fn glued_goes_along() {
        let glue = glue_of(MAIN, [900, 150, 1200, 500]).unwrap();
        // The player moved and grew.
        assert_eq!(
            placed([0, 0, 1000, 700], [300, 350], glue),
            [1000, 50, 1300, 400]
        );
        let below = Glue {
            side: Side::Bottom,
            offset: 10,
        };
        assert_eq!(placed(MAIN, [400, 200], below), [110, 600, 510, 800]);
        let above = Glue {
            side: Side::Top,
            offset: 0,
        };
        assert_eq!(placed(MAIN, [400, 200], above), [100, -100, 500, 100]);
    }

    #[test]
    fn magnet() {
        // Near the right edge and the top: beside it, tops in line.
        assert_eq!(
            snapped(MAIN, [906, 95, 1206, 495], 8),
            [900, 100, 1200, 500]
        );
        // Near the left edge, bottoms in line.
        assert_eq!(
            snapped(MAIN, [-195, 205, 95, 603], 8),
            [-190, 202, 100, 600]
        );
        // Under it, lefts in line.
        assert_eq!(snapped(MAIN, [104, 607, 504, 807], 8), [100, 600, 500, 800]);
        // Too far, or beside it but past its end.
        assert_eq!(snapped(MAIN, [920, 95, 1220, 495], 8), [920, 95, 1220, 495]);
        assert_eq!(
            snapped(MAIN, [905, 700, 1205, 900], 8),
            [905, 700, 1205, 900]
        );
    }

    #[test]
    fn first_on_the_right() {
        assert_eq!(first_place(MAIN, 300), [900, 100, 1200, 600]);
        assert!(glue_of(MAIN, first_place(MAIN, 300)).is_some());
    }
}
