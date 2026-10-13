//! The picture's adjustments of the Video, sound and subtitles panel: the colour
//! (mpv's video equalizer, done by the renderer on the GPU), the rotation
//! (`video-rotate`, also on the GPU), the flips, the sharpness and the
//! noise reduction (FFmpeg filters in `vf`, on the processor) and the
//! debanding (`deband`, on the GPU). They last while the program runs,
//! over every file, and are not saved.
//!
//! mpv does not take frames out of the graphics card for a lavfi filter
//! that cannot use them (the filter fails and is disabled), so while there
//! are filters the player decodes with copying (`Player::adjust`). Upside
//! down is `hflip` and half a turn: FFmpeg's `vflip` takes CUDA frames as
//! they are, with a negative stride the CUDA interop refuses (a blue
//! picture); both flips together are half a turn alone.

/// The colour's properties, each from -100 to 100 (0: as the file is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Brightness,
    Contrast,
    Saturation,
    Hue,
    Gamma,
}

impl Color {
    pub const ALL: [Color; 5] = [
        Color::Brightness,
        Color::Contrast,
        Color::Saturation,
        Color::Hue,
        Color::Gamma,
    ];

    /// mpv's property.
    pub fn property(self) -> &'static str {
        match self {
            Color::Brightness => "brightness",
            Color::Contrast => "contrast",
            Color::Saturation => "saturation",
            Color::Hue => "hue",
            Color::Gamma => "gamma",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Color::Brightness => tr!("Brightness", "Яркость"),
            Color::Contrast => tr!("Contrast", "Контраст"),
            Color::Saturation => tr!("Saturation", "Насыщенность"),
            Color::Hue => tr!("Hue", "Оттенок"),
            Color::Gamma => tr!("Gamma", "Гамма"),
        }
    }
}

/// The picture's shape: the file's, or one forced on it
/// (`video-aspect-override`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Aspect {
    #[default]
    Auto,
    R4x3,
    R16x9,
    R16x10,
    R185,
    R235,
    R239,
}

impl Aspect {
    pub const ALL: [Aspect; 7] = [
        Aspect::Auto,
        Aspect::R4x3,
        Aspect::R16x9,
        Aspect::R16x10,
        Aspect::R185,
        Aspect::R235,
        Aspect::R239,
    ];

    /// mpv's `video-aspect-override` (-1: the file's).
    pub fn option(self) -> &'static str {
        match self {
            Aspect::Auto => "-1",
            Aspect::R4x3 => "4:3",
            Aspect::R16x9 => "16:9",
            Aspect::R16x10 => "16:10",
            Aspect::R185 => "1.85:1",
            Aspect::R235 => "2.35:1",
            Aspect::R239 => "2.39:1",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Aspect::Auto => tr!("As in the file", "Как в файле"),
            other => other.option(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adjust {
    /// By [`Color::ALL`]'s order.
    pub color: [i64; 5],
    /// Clockwise, 0, 90, 180 or 270.
    pub rotate: i64,
    /// Upside down.
    pub flip: bool,
    /// Left to right.
    pub mirror: bool,
    /// 0 to 100 (CAS' strength in percent).
    pub sharpen: i64,
    pub denoise: bool,
    pub deband: bool,
    pub aspect: Aspect,
}

impl Adjust {
    pub fn get(&self, c: Color) -> i64 {
        self.color[c as usize]
    }

    pub fn set(&mut self, c: Color, value: i64) {
        self.color[c as usize] = value.clamp(-100, 100);
    }

    /// Turn by `quarters` quarter turns clockwise (negative: anticlockwise).
    pub fn turn(&mut self, quarters: i64) {
        self.rotate = (self.rotate + quarters * 90).rem_euclid(360);
    }

    /// mpv's `vf`: the filters in the order they act, empty for none.
    pub fn filters(&self) -> String {
        let mut vf = Vec::new();
        if self.denoise {
            vf.push("lavfi=[hqdn3d]".to_owned());
        }
        if self.sharpen > 0 {
            vf.push(format!(
                "lavfi=[cas=strength={:.2}]",
                self.sharpen.clamp(0, 100) as f64 / 100.0
            ));
        }
        // Upside down is mirrored and turned half round (`rotation`).
        if self.flip != self.mirror {
            vf.push("lavfi=[hflip]".to_owned());
        }
        vf.join(",")
    }

    /// mpv's `video-rotate`: the turn, and half a turn more upside down.
    pub fn rotation(&self) -> i64 {
        (self.rotate + if self.flip { 180 } else { 0 }).rem_euclid(360)
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_wrap() {
        let mut a = Adjust::default();
        a.turn(-1);
        assert_eq!(a.rotate, 270);
        a.turn(2);
        assert_eq!(a.rotate, 90);
        a.turn(3);
        assert_eq!(a.rotate, 0);
    }

    #[test]
    fn filters_in_order() {
        let mut a = Adjust::default();
        assert_eq!(a.filters(), "");
        a.mirror = true;
        a.sharpen = 50;
        a.denoise = true;
        assert_eq!(
            a.filters(),
            "lavfi=[hqdn3d],lavfi=[cas=strength=0.50],lavfi=[hflip]"
        );
        a.set(Color::Gamma, 300);
        assert_eq!(a.get(Color::Gamma), 100);
        assert!(!a.is_default());
    }

    #[test]
    fn flips_are_a_mirror_and_half_a_turn() {
        let mut a = Adjust::default();
        a.turn(1);
        a.flip = true;
        assert_eq!((a.filters().as_str(), a.rotation()), ("lavfi=[hflip]", 270));
        a.mirror = true;
        assert_eq!((a.filters().as_str(), a.rotation()), ("", 270));
        a.flip = false;
        assert_eq!((a.filters().as_str(), a.rotation()), ("lavfi=[hflip]", 90));
    }
}
