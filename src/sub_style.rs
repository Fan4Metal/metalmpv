//! How subtitles look (the panel's Subtitles tab and the settings): the
//! font family and the size, the colour of the letters and of their
//! outline, the outline's thickness and the height on the picture. mpv's
//! `sub-font`, `sub-scale`, `sub-color`, `sub-outline-color`,
//! `sub-outline-size` and `sub-pos`. The size and the height apply to all
//! subtitles; the rest only to text ones (SRT and the like): ASS subtitles
//! keep their own styles. Kept between runs.

use std::ops::RangeInclusive;

/// The size, in percent of mpv's.
pub const SCALE_RANGE: RangeInclusive<f64> = 50.0..=300.0;
/// The outline's thickness, in mpv's units (scaled pixels at 720 lines).
pub const OUTLINE_RANGE: RangeInclusive<f64> = 0.0..=6.0;
/// The height: 100 at the bottom (mpv's `sub-pos`), less higher up.
pub const POS_RANGE: RangeInclusive<f64> = 50.0..=100.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SubStyle {
    /// The font family, empty for mpv's (`sans-serif`).
    pub font: String,
    pub scale: f64,
    pub color: [u8; 3],
    pub outline_color: [u8; 3],
    pub outline: f64,
    pub pos: f64,
}

impl Default for SubStyle {
    /// mpv's own.
    fn default() -> Self {
        Self {
            font: String::new(),
            scale: 100.0,
            color: [255, 255, 255],
            outline_color: [0, 0, 0],
            outline: 1.65,
            pos: 100.0,
        }
    }
}

impl SubStyle {
    /// mpv's properties and their values.
    pub fn options(&self) -> [(&'static str, String); 6] {
        let font = if self.font.is_empty() {
            "sans-serif"
        } else {
            &self.font
        };
        [
            ("sub-font", font.to_owned()),
            ("sub-scale", format!("{:.2}", self.scale / 100.0)),
            ("sub-color", hex(self.color)),
            ("sub-outline-color", hex(self.outline_color)),
            ("sub-outline-size", format!("{:.2}", self.outline)),
            ("sub-pos", format!("{:.0}", self.pos)),
        ]
    }
}

/// `#RRGGBB`.
pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// A colour written by [`hex`].
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip() {
        assert_eq!(hex([255, 200, 0]), "#FFC800");
        assert_eq!(parse_hex("#FFC800"), Some([255, 200, 0]));
        assert_eq!(parse_hex("#ffc800"), Some([255, 200, 0]));
        assert_eq!(parse_hex("FFC800"), None);
        assert_eq!(parse_hex("#FFC8"), None);
        assert_eq!(parse_hex("#ЖЖЖ"), None);
    }

    #[test]
    fn options_for_mpv() {
        let style = SubStyle {
            font: "Arial".into(),
            scale: 150.0,
            color: [255, 255, 0],
            outline: 3.0,
            pos: 90.0,
            ..SubStyle::default()
        };
        assert_eq!(
            style.options(),
            [
                ("sub-font", "Arial".to_owned()),
                ("sub-scale", "1.50".to_owned()),
                ("sub-color", "#FFFF00".to_owned()),
                ("sub-outline-color", "#000000".to_owned()),
                ("sub-outline-size", "3.00".to_owned()),
                ("sub-pos", "90".to_owned()),
            ]
        );
        assert_eq!(SubStyle::default().options()[0].1, "sans-serif");
    }
}
