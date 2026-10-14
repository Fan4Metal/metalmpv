//! The picture's adjustments of the Video, sound and subtitles panel: the colour
//! (mpv's video equalizer, done by the renderer on the GPU), the rotation
//! (`video-rotate`, also on the GPU), the flips, the sharpness and the
//! noise reduction (FFmpeg filters in `vf`, on the processor) and the
//! debanding (`deband`, on the GPU), the zoom and the pan (`video-zoom`,
//! `video-pan-x/y`), the deinterlacing (`deinterlace`) and how HDR is
//! brought down to an ordinary screen (`tone-mapping`). They last over
//! every file while the program runs, and between runs when the panel's
//! switch says so (`settings.keep_adjust`, [`Adjust::to_text`]).
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

/// Interlaced pictures woven back into whole frames (`deinterlace`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Deinterlace {
    /// Frames marked interlaced only.
    #[default]
    Auto,
    On,
    Off,
}

impl Deinterlace {
    pub const ALL: [Deinterlace; 3] = [Deinterlace::Auto, Deinterlace::On, Deinterlace::Off];

    /// mpv's `deinterlace`, also the stored name.
    pub fn option(self) -> &'static str {
        match self {
            Deinterlace::Auto => "auto",
            Deinterlace::On => "yes",
            Deinterlace::Off => "no",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Deinterlace::Auto => tr!("Auto", "Авто"),
            Deinterlace::On => tr!("On", "Вкл."),
            Deinterlace::Off => tr!("Off", "Выкл."),
        }
    }
}

/// How an HDR picture is brought down to an ordinary screen
/// (`tone-mapping`); an HDR screen takes it as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneMap {
    #[default]
    Auto,
    Bt2390,
    Hable,
    Mobius,
    Reinhard,
    Clip,
}

impl ToneMap {
    pub const ALL: [ToneMap; 6] = [
        ToneMap::Auto,
        ToneMap::Bt2390,
        ToneMap::Hable,
        ToneMap::Mobius,
        ToneMap::Reinhard,
        ToneMap::Clip,
    ];

    /// mpv's `tone-mapping`, also the stored name.
    pub fn option(self) -> &'static str {
        match self {
            ToneMap::Auto => "auto",
            ToneMap::Bt2390 => "bt.2390",
            ToneMap::Hable => "hable",
            ToneMap::Mobius => "mobius",
            ToneMap::Reinhard => "reinhard",
            ToneMap::Clip => "clip",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ToneMap::Auto => tr!("Auto", "Авто"),
            ToneMap::Bt2390 => tr!("BT.2390, the standard's", "BT.2390, по стандарту"),
            ToneMap::Hable => tr!("Hable, filmic", "Hable, кинематографичный"),
            ToneMap::Mobius => tr!("Möbius, bright", "Möbius, яркий"),
            ToneMap::Reinhard => tr!("Reinhard, soft", "Reinhard, мягкий"),
            ToneMap::Clip => tr!("Clip, no compression", "Обрезка, без компрессии"),
        }
    }
}

/// The zoom, in percent of the picture as it fits.
pub const ZOOM_RANGE: std::ops::RangeInclusive<i64> = 50..=400;
/// The pan, in percent of the picture's width or height either way.
pub const PAN_RANGE: std::ops::RangeInclusive<i64> = -50..=50;

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

#[derive(Clone, Debug, PartialEq)]
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
    /// 0 to 100: hqdn3d's strengths in proportion, 40 near its defaults.
    pub denoise: i64,
    pub deband: bool,
    pub aspect: Aspect,
    /// In percent, 100 as the picture fits ([`ZOOM_RANGE`]).
    pub zoom: i64,
    /// In percent of the picture's size, right and down positive
    /// ([`PAN_RANGE`]).
    pub pan_x: i64,
    pub pan_y: i64,
    pub deinterlace: Deinterlace,
    pub tone_map: ToneMap,
}

impl Default for Adjust {
    fn default() -> Self {
        Self {
            color: [0; 5],
            rotate: 0,
            flip: false,
            mirror: false,
            sharpen: 0,
            denoise: 0,
            deband: false,
            aspect: Aspect::Auto,
            zoom: 100,
            pan_x: 0,
            pan_y: 0,
            deinterlace: Deinterlace::Auto,
            tone_map: ToneMap::Auto,
        }
    }
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
        if self.denoise > 0 {
            // FFmpeg's defaults are 4:3:6:4.5, spatial and temporal, the
            // luma's and the chroma's.
            let luma = self.denoise.clamp(0, 100) as f64 / 10.0;
            vf.push(format!(
                "lavfi=[hqdn3d={luma:.2}:{:.2}:{:.2}:{:.2}]",
                luma * 0.75,
                luma * 1.5,
                luma * 1.125
            ));
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

    /// mpv's `video-zoom`: a power of two.
    pub fn zoom_option(&self) -> f64 {
        (self.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()) as f64 / 100.0).log2()
    }

    /// The setting: `name=value` pairs apart, what differs from the
    /// defaults.
    pub fn to_text(&self) -> String {
        let d = Self::default();
        let mut parts = Vec::new();
        for c in Color::ALL {
            if self.get(c) != 0 {
                parts.push(format!("{}={}", c.property(), self.get(c)));
            }
        }
        let numbers = [
            ("rotate", self.rotate, d.rotate),
            ("sharpen", self.sharpen, d.sharpen),
            ("denoise", self.denoise, d.denoise),
            ("zoom", self.zoom, d.zoom),
            ("pan_x", self.pan_x, d.pan_x),
            ("pan_y", self.pan_y, d.pan_y),
        ];
        for (name, value, default) in numbers {
            if value != default {
                parts.push(format!("{name}={value}"));
            }
        }
        for (name, on) in [
            ("flip", self.flip),
            ("mirror", self.mirror),
            ("deband", self.deband),
        ] {
            if on {
                parts.push(format!("{name}=yes"));
            }
        }
        if self.aspect != d.aspect {
            parts.push(format!("aspect={}", self.aspect.option()));
        }
        if self.deinterlace != d.deinterlace {
            parts.push(format!("deinterlace={}", self.deinterlace.option()));
        }
        if self.tone_map != d.tone_map {
            parts.push(format!("tone_map={}", self.tone_map.option()));
        }
        parts.join(" ")
    }

    /// The adjustments of the setting; what is not understood keeps its
    /// default, numbers kept within their ranges.
    pub fn from_text(text: &str) -> Self {
        let mut a = Self::default();
        for (name, value) in text.split_whitespace().filter_map(|p| p.split_once('=')) {
            let number = value.parse::<i64>().ok();
            if let Some(c) = Color::ALL.into_iter().find(|c| c.property() == name) {
                if let Some(n) = number {
                    a.set(c, n);
                }
                continue;
            }
            match (name, number) {
                ("rotate", Some(n)) => a.rotate = (n / 90 * 90).rem_euclid(360),
                ("sharpen", Some(n)) => a.sharpen = n.clamp(0, 100),
                ("denoise", Some(n)) => a.denoise = n.clamp(0, 100),
                ("zoom", Some(n)) => a.zoom = n.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()),
                ("pan_x", Some(n)) => a.pan_x = n.clamp(*PAN_RANGE.start(), *PAN_RANGE.end()),
                ("pan_y", Some(n)) => a.pan_y = n.clamp(*PAN_RANGE.start(), *PAN_RANGE.end()),
                ("flip", _) => a.flip = value == "yes",
                ("mirror", _) => a.mirror = value == "yes",
                ("deband", _) => a.deband = value == "yes",
                ("aspect", _) => {
                    if let Some(x) = Aspect::ALL.into_iter().find(|x| x.option() == value) {
                        a.aspect = x;
                    }
                }
                ("deinterlace", _) => {
                    if let Some(x) = Deinterlace::ALL.into_iter().find(|x| x.option() == value) {
                        a.deinterlace = x;
                    }
                }
                ("tone_map", _) => {
                    if let Some(x) = ToneMap::ALL.into_iter().find(|x| x.option() == value) {
                        a.tone_map = x;
                    }
                }
                _ => {}
            }
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        assert_eq!(Adjust::default().to_text(), "");
        let mut a = Adjust::default();
        a.set(Color::Brightness, 12);
        a.set(Color::Hue, -30);
        a.turn(1);
        a.flip = true;
        a.denoise = 40;
        a.zoom = 150;
        a.pan_y = -10;
        a.aspect = Aspect::R235;
        a.deinterlace = Deinterlace::On;
        a.tone_map = ToneMap::Hable;
        let text = a.to_text();
        assert_eq!(
            text,
            "brightness=12 hue=-30 rotate=90 denoise=40 zoom=150 pan_y=-10 flip=yes aspect=2.35:1 deinterlace=yes tone_map=hable"
        );
        assert_eq!(Adjust::from_text(&text), a);
        let odd = Adjust::from_text("zoom=9000 rotate=45 gamma=x tone_map=nope pan_x=-80 junk");
        assert_eq!(odd.zoom, 400);
        assert_eq!(odd.rotate, 0);
        assert_eq!(odd.pan_x, -50);
        assert_eq!(odd.tone_map, ToneMap::Auto);
        assert!((Adjust::default().zoom_option()).abs() < 1e-9);
        let z = Adjust {
            zoom: 200,
            ..Adjust::default()
        };
        assert!((z.zoom_option() - 1.0).abs() < 1e-9);
    }

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
        a.denoise = 40;
        assert_eq!(
            a.filters(),
            "lavfi=[hqdn3d=4.00:3.00:6.00:4.50],lavfi=[cas=strength=0.50],lavfi=[hflip]"
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
