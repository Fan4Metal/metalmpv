//! The window's size for the picture (`1`, `2`, `3`): the client area that
//! shows a picture of so many pixels with the title row, the control bar
//! and the playlist (the chrome) around it, kept on the monitor's work
//! area. All in pixels; `win.rs` moves the window.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_letterboxes() {
        let [w, h] = fitted([1920.0, 800.0], [1000.0, 600.0]);
        assert_eq!([w.round(), (h * 100.0).round()], [1000.0, 41667.0]);
        assert_eq!(fitted([1080.0, 1920.0], [1000.0, 960.0]), [540.0, 960.0]);
        assert_eq!(fitted([0.0, 0.0], [10.0, 20.0]), [10.0, 20.0]);
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
