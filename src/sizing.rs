//! The window's size for the picture (`1`, `2`, `3`): the client area that
//! shows a picture of so many pixels with the title row, the control bar
//! and the playlist (the chrome) around it, kept on the monitor's work
//! area. All in pixels; `win.rs` moves the window.

/// The picture `video` as wide as `area`, its height by its shape: a file
/// opened keeps the window's width, whatever the shape of the file before
/// (`fitted` would shrink the window at each change of shape).
pub fn same_width(video: [f64; 2], area: [f64; 2]) -> [f64; 2] {
    if video[0] <= 0.0 || video[1] <= 0.0 || area[0] <= 0.0 {
        return area;
    }
    [area[0], area[0] * video[1] / video[0]]
}

/// The size of the picture `video` (its display size) shown whole in
/// `area`, as mpv letterboxes it.
pub fn fitted(video: [f64; 2], area: [f64; 2]) -> [f64; 2] {
    if video[0] <= 0.0 || video[1] <= 0.0 {
        return area;
    }
    let scale = (area[0] / video[0]).min(area[1] / video[1]);
    [video[0] * scale, video[1] * scale]
}

/// The client area that shows a picture of `picture` pixels with `chrome`
/// pixels around it. The picture is scaled down, keeping its shape, when
/// the window would not fit in `work`; the window is not made smaller
/// than `min`.
pub fn client_size(picture: [f64; 2], chrome: [f64; 2], work: [f64; 2], min: [f64; 2]) -> [i32; 2] {
    let room = [work[0] - chrome[0], work[1] - chrome[1]];
    let scale = (room[0] / picture[0])
        .min(room[1] / picture[1])
        .clamp(0.0, 1.0);
    let size = |i: usize| {
        (picture[i] * scale + chrome[i])
            .round()
            .max(min[i].round())
            .min(work[i]) as i32
    };
    [size(0), size(1)]
}

/// A rectangle (left, top, right, bottom) of `size` centred where
/// `current` is, moved inside `work` as far as it fits.
pub fn placed(current: [i32; 4], size: [i32; 2], work: [i32; 4]) -> [i32; 4] {
    let centre = |a: i32, b: i32, len: i32| a + (b - a - len) / 2;
    let inside = |pos: i32, len: i32, lo: i32, hi: i32| pos.min(hi - len).max(lo);
    let left = inside(
        centre(current[0], current[2], size[0]),
        size[0],
        work[0],
        work[2],
    );
    let top = inside(
        centre(current[1], current[3], size[1]),
        size[1],
        work[1],
        work[3],
    );
    [left, top, left + size[0], top + size[1]]
}

/// `rect` (a window being moved) with its centre on the centre of `work`
/// along each axis where it is within `reach` pixels of it.
pub fn snapped_to_centre(rect: [i32; 4], work: [i32; 4], reach: i32) -> [i32; 4] {
    let shift = |a: i32, b: i32, lo: i32, hi: i32| {
        // Doubled: the centres of odd sizes stay whole numbers.
        let d = (lo + hi) - (a + b);
        if d.abs() <= 2 * reach { d / 2 } else { 0 }
    };
    let dx = shift(rect[0], rect[2], work[0], work[2]);
    let dy = shift(rect[1], rect[3], work[1], work[3]);
    [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]
}

/// `rect` (a window being moved) put against each edge of `work` it comes
/// within `reach` pixels of: the mini player's magnet.
pub fn snapped_to_edges(rect: [i32; 4], work: [i32; 4], reach: i32) -> [i32; 4] {
    let shift = |a: i32, b: i32, lo: i32, hi: i32| {
        if (lo - a).abs() <= reach {
            lo - a
        } else if (hi - b).abs() <= reach {
            hi - b
        } else {
            0
        }
    };
    let dx = shift(rect[0], rect[2], work[0], work[2]);
    let dy = shift(rect[1], rect[3], work[1], work[3]);
    [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]
}

/// The room the mini player's controls take across, in points.
const MINI_CONTROLS: f64 = 180.0;
/// The mini player's shorter side at its least, in points.
const MINI_SHORT: f64 = 100.0;

/// The mini player's least client size in points for a picture of
/// `aspect` (width over height; none for sound alone): as wide as its
/// controls, the picture's shorter side at least [`MINI_SHORT`].
pub fn mini_least(aspect: Option<f64>) -> [f64; 2] {
    match aspect.filter(|a| *a > 0.0) {
        Some(a) => {
            let width = (MINI_SHORT * a.max(1.0)).max(MINI_CONTROLS);
            [width, width / a]
        }
        None => [MINI_CONTROLS, MINI_SHORT],
    }
}

/// The mini player at first: `size` in the bottom right corner of `work`,
/// `margin` pixels off its edges.
pub fn mini_corner(size: [i32; 2], work: [i32; 4], margin: i32) -> [i32; 4] {
    let right = work[2] - margin;
    let bottom = work[3] - margin;
    [right - size[0], bottom - size[1], right, bottom]
}

/// The mini player's window `rect` for a picture of `aspect` (of its
/// client area, `border` pixels less across and down): its width kept and
/// its height by the shape, not under `least` nor over `work`; the edges
/// nearer the work area's edges stay where they are (a window in the bottom
/// right corner stays in it), and it is kept on the work area.
pub fn mini_fitted(
    rect: [i32; 4],
    aspect: f64,
    border: [i32; 2],
    least: [i32; 2],
    work: [i32; 4],
) -> [i32; 4] {
    let room = [
        f64::from(work[2] - work[0] - border[0]),
        f64::from(work[3] - work[1] - border[1]),
    ];
    let mut width = f64::from((rect[2] - rect[0] - border[0]).max(least[0]));
    let mut height = width / aspect;
    if height < f64::from(least[1]) {
        height = f64::from(least[1]);
        width = height * aspect;
    }
    if width > room[0] {
        width = room[0];
        height = width / aspect;
    }
    if height > room[1] {
        height = room[1];
        width = height * aspect;
    }
    let size = [
        width.round() as i32 + border[0],
        height.round() as i32 + border[1],
    ];
    // The centres compared doubled: whole numbers for odd sizes.
    let start = |axis: usize| {
        if rect[axis] + rect[axis + 2] > work[axis] + work[axis + 2] {
            rect[axis + 2] - size[axis]
        } else {
            rect[axis]
        }
    };
    let inside = |pos: i32, len: i32, lo: i32, hi: i32| pos.min(hi - len).max(lo);
    let left = inside(start(0), size[0], work[0], work[2]);
    let top = inside(start(1), size[1], work[1], work[3]);
    [left, top, left + size[0], top + size[1]]
}

/// `rect` (a window being sized by its `side`: -1 the left or top edge,
/// 1 the right or bottom one, 0 neither, across and down) with its client
/// area (`border` pixels less) of `aspect`. Dragged by a side edge or a
/// corner, the width counts and the edge across from the one dragged stays;
/// by the top or the bottom edge, the height counts.
pub fn kept_aspect(rect: [i32; 4], side: [i32; 2], aspect: f64, border: [i32; 2]) -> [i32; 4] {
    let [left, mut top, mut right, mut bottom] = rect;
    if side[0] != 0 {
        let width = f64::from(right - left - border[0]);
        let height = (width / aspect).round() as i32 + border[1];
        if side[1] < 0 {
            top = bottom - height;
        } else {
            bottom = top + height;
        }
    } else {
        let height = f64::from(bottom - top - border[1]);
        let width = (height * aspect).round() as i32 + border[0];
        right = left + width;
    }
    [left, top, right, bottom]
}

/// `window` (pixels) grown or shrunk from insets `old` to `new` (points,
/// left, top, right, bottom), the picture keeping its place; not under
/// `least` (pixels): what a shrink could not take off is added to `slack`,
/// and a growth takes it off first.
pub fn refit(
    window: [i32; 4],
    old: [f32; 4],
    new: [f32; 4],
    ppp: f32,
    least: [i32; 2],
    slack: &mut [i32; 2],
) -> [i32; 4] {
    let grow = |i: usize| (new[i] * ppp).round() as i32 - (old[i] * ppp).round() as i32;
    let mut size = |axis: usize, change: i32| {
        let mut size = window[axis + 2] - window[axis] + change;
        if change > 0 {
            let taken = slack[axis].min(change);
            size -= taken;
            slack[axis] -= taken;
        }
        if size < least[axis] {
            slack[axis] += least[axis] - size;
            size = least[axis];
        }
        size
    };
    let (width, height) = (size(0, grow(0) + grow(2)), size(1, grow(1) + grow(3)));
    let (left, top) = (window[0] - grow(0), window[1] - grow(1));
    [left, top, left + width, top + height]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mini_player_holds_to_the_edges() {
        let work = [0, 0, 1920, 1040];
        // 10 px off the right edge and 30 off the bottom: drawn right.
        assert_eq!(
            snapped_to_edges([1510, 700, 1910, 1010], work, 16),
            [1520, 700, 1920, 1010]
        );
        // Near the top left corner: both.
        assert_eq!(
            snapped_to_edges([5, -4, 405, 221], work, 16),
            [0, 0, 400, 225]
        );
        assert_eq!(
            snapped_to_edges([500, 500, 900, 725], work, 16),
            [500, 500, 900, 725]
        );
    }

    #[test]
    fn the_mini_player_is_wide_enough_for_its_controls() {
        assert_eq!(mini_least(Some(16.0 / 9.0)), [180.0, 101.25]);
        // A wide film: its height counts.
        let [w, h] = mini_least(Some(2.4));
        assert_eq!((w, h.round()), (240.0, 100.0));
        // Upright: as wide as the controls.
        assert_eq!(mini_least(Some(0.5)), [180.0, 360.0]);
        assert_eq!(mini_least(None), [180.0, 100.0]);
    }

    #[test]
    fn the_mini_player_takes_the_pictures_shape_in_its_corner() {
        let work = [0, 0, 1920, 1040];
        let corner = mini_corner([384, 216], work, 16);
        assert_eq!(corner, [1520, 808, 1904, 1024]);
        // A 4:3 file: taller, the bottom right corner kept.
        assert_eq!(
            mini_fitted(corner, 4.0 / 3.0, [0, 0], [180, 100], work),
            [1520, 736, 1904, 1024]
        );
        // In the top left corner the top left one stays.
        assert_eq!(
            mini_fitted([0, 0, 384, 216], 4.0 / 3.0, [0, 0], [180, 100], work),
            [0, 0, 384, 288]
        );
        // Upright, too tall for the screen: as tall as it.
        assert_eq!(
            mini_fitted([0, 0, 1000, 200], 0.5, [0, 0], [180, 100], work),
            [0, 0, 520, 1040]
        );
        // Not under the least size, with a frame around.
        assert_eq!(
            mini_fitted([0, 0, 100, 50], 16.0 / 9.0, [2, 2], [180, 100], work),
            [0, 0, 182, 103]
        );
    }

    #[test]
    fn a_mini_player_sized_keeps_its_shape() {
        let rect = [100, 100, 500, 325];
        // The right edge dragged out: the bottom follows.
        assert_eq!(
            kept_aspect([100, 100, 580, 325], [1, 0], 16.0 / 9.0, [0, 0]),
            [100, 100, 580, 370]
        );
        // The top left corner: the bottom right one stays.
        assert_eq!(
            kept_aspect([20, 60, 500, 325], [-1, -1], 16.0 / 9.0, [0, 0]),
            [20, 55, 500, 325]
        );
        // The bottom edge: the width follows.
        assert_eq!(
            kept_aspect([100, 100, 500, 280], [0, 1], 2.0, [0, 0]),
            [100, 100, 460, 280]
        );
        assert_eq!(kept_aspect(rect, [0, -1], 400.0 / 225.0, [0, 0]), rect);
    }

    #[test]
    fn refit_comes_back_at_the_least_size() {
        // Full with a docked playlist (300 across, title 32 and bar 80) to
        // only the picture and back, in a small window.
        let full = [0.0, 32.0, 300.0, 80.0];
        let bare = [0.0; 4];
        let least = [560, 340];
        let window = [100, 100, 700, 600];
        let mut slack = [0, 0];
        let small = refit(window, full, bare, 1.0, least, &mut slack);
        assert_eq!(small, [100, 132, 660, 520]);
        assert_eq!(slack, [260, 0]);
        let back = refit(small, bare, full, 1.0, least, &mut slack);
        assert_eq!(back, window);
        assert_eq!(slack, [0, 0]);
        // Room to spare: no slack.
        let big = [0, 0, 1200, 900];
        let mut slack = [0, 0];
        let less = refit(big, full, bare, 2.0, least, &mut slack);
        assert_eq!(less, [0, 64, 600, 740]);
        assert_eq!(refit(less, bare, full, 2.0, least, &mut slack), big);
    }

    #[test]
    fn snaps_near_the_centre_only() {
        let work = [0, 0, 1920, 1040];
        // 10 px right of the centre, 100 px above it: x snaps, y stays.
        assert_eq!(
            snapped_to_centre([570, 320, 1370, 420], work, 16),
            [560, 320, 1360, 420]
        );
        // Both within reach.
        assert_eq!(
            snapped_to_centre([548, 260, 1348, 760], work, 16),
            [560, 270, 1360, 770]
        );
        // Out of reach: as it was.
        assert_eq!(
            snapped_to_centre([600, 0, 1400, 500], work, 16),
            [600, 0, 1400, 500]
        );
        // A second monitor to the left.
        assert_eq!(
            snapped_to_centre([-1350, 100, -550, 600], [-1920, 0, 0, 1040], 16),
            [-1360, 100, -560, 600]
        );
    }

    #[test]
    fn fitted_letterboxes() {
        let [w, h] = fitted([1920.0, 800.0], [1000.0, 600.0]);
        assert_eq!([w.round(), (h * 100.0).round()], [1000.0, 41667.0]);
        assert_eq!(fitted([1080.0, 1920.0], [1000.0, 960.0]), [540.0, 960.0]);
        assert_eq!(fitted([0.0, 0.0], [10.0, 20.0]), [10.0, 20.0]);
    }

    #[test]
    fn files_of_other_shapes_keep_the_width() {
        // 2.39:1, 4:3, 16:9 in turn: the width stays, the window does not
        // shrink step by step.
        let mut area = [1280.0, 720.0];
        for video in [[1920.0, 803.0], [1440.0, 1080.0], [1920.0, 1080.0]] {
            area = same_width(video, area);
            assert_eq!(area[0], 1280.0);
        }
        assert_eq!(area, [1280.0, 720.0]);
        assert_eq!(same_width([0.0, 0.0], [10.0, 20.0]), [10.0, 20.0]);
    }

    #[test]
    fn client_holds_the_picture_and_the_chrome() {
        let work = [2560.0, 1400.0];
        let min = [560.0, 340.0];
        // 100 % of 1080p with a title row and a control bar.
        assert_eq!(
            client_size([1920.0, 1080.0], [0.0, 120.0], work, min),
            [1920, 1200]
        );
        // Too tall for the work area: scaled down, the shape kept.
        assert_eq!(
            client_size([3840.0, 2160.0], [0.0, 120.0], work, min),
            [(1280.0f64 * 16.0 / 9.0).round() as i32, 1400]
        );
        // Never under the minimum.
        assert_eq!(
            client_size([320.0, 180.0], [0.0, 120.0], work, min),
            [560, 340]
        );
    }

    #[test]
    fn placed_keeps_the_centre_inside_the_work_area() {
        let work = [0, 0, 1920, 1040];
        assert_eq!(
            placed([100, 100, 900, 600], [400, 300], work),
            [300, 200, 700, 500]
        );
        // Pushed back from the right and the bottom edges.
        assert_eq!(
            placed([1500, 800, 1900, 1000], [800, 600], work),
            [1120, 440, 1920, 1040]
        );
        // Bigger than the work area: its top left corner stays on it.
        assert_eq!(placed([0, 0, 10, 10], [2000, 1100], work)[..2], [0, 0]);
    }
}
