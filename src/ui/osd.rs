//! The on-screen display: a line of yellow text in the top left corner of
//! the picture after a seek, a volume change and the like, gone after a
//! moment ("[ Назад 10 сек / Ключевой кадр / 00:01:23 (12,5%) ]"). Its look
//! is the user's ([`OsdStyle`]), as the subtitles' is.

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use egui::{Color32, FontId, Rect, Vec2, vec2};

use super::style;
use crate::format::Osd;
use crate::player::State;

/// The outline's thickness and the shadow's offset, in points of a line
/// 22 points high (they grow with the line).
pub const OUTLINE_RANGE: RangeInclusive<f64> = 0.0..=4.0;
pub const SHADOW_RANGE: RangeInclusive<f64> = 0.0..=4.0;
/// The opacity of the box behind the text, of 255 (the subtitles' too).
const BOX_ALPHA: u8 = 0xc0;
/// The size the outline and the shadow are given for.
const UNIT_SIZE: f32 = 22.0;

/// How the OSD line looks: the letters' colour, the outline's colour and
/// thickness, the shadow, boldness, and a box of the outline's colour in
/// place of the outline. Kept in the settings.
#[derive(Clone, Debug, PartialEq)]
pub struct OsdStyle {
    pub color: [u8; 3],
    pub outline_color: [u8; 3],
    /// The outline's thickness; with `boxed`, the box's margin.
    pub outline: f64,
    pub shadow: f64,
    pub bold: bool,
    pub boxed: bool,
}

impl Default for OsdStyle {
    /// Yellow with a thin black outline and a shadow.
    fn default() -> Self {
        let c = style::OSD;
        Self {
            color: [c.r(), c.g(), c.b()],
            outline_color: [0, 0, 0],
            outline: 1.0,
            shadow: 1.0,
            bold: false,
            boxed: false,
        }
    }
}

pub struct OsdLine {
    line: Option<(Osd, Instant)>,
    /// How long a line stays whole, and how long it then fades (the
    /// settings' `osd_duration` and `osd_fade`).
    shown: Duration,
    fade: Duration,
    /// Shown, without fading, while playback stays stopped at the end of
    /// the file (why it stopped: the data ran out); a passing line covers
    /// it for its while.
    at_end: Option<Osd>,
}

impl Default for OsdLine {
    fn default() -> Self {
        Self {
            line: None,
            shown: Duration::from_millis(1600),
            fade: Duration::from_millis(400),
            at_end: None,
        }
    }
}

impl OsdLine {
    /// How long a line stays whole and fades, in seconds.
    pub fn set_timing(&mut self, shown: f64, fade: f64) {
        self.shown = Duration::from_secs_f64(shown.max(0.0));
        self.fade = Duration::from_secs_f64(fade.max(0.0));
    }

    pub fn show(&mut self, osd: Osd) {
        self.line = Some((osd, Instant::now()));
    }

    /// Show `osd` until playback leaves the end of the file.
    pub fn show_at_end(&mut self, osd: Osd) {
        self.at_end = Some(osd);
        self.line = None;
    }

    pub fn clear_at_end(&mut self) {
        self.at_end = None;
    }

    /// Paint the line over `video` in `look`, `scale` times its usual size
    /// (which follows the picture's height). A seek's line shows where
    /// playback actually landed (a keyframe seek lands near, not on, the
    /// target) once mpv has finished the seek. With `passing` off (the OSD
    /// turned off) only the line at the end is shown.
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        video: Rect,
        state: &State,
        look: &OsdStyle,
        scale: f32,
        passing: bool,
    ) {
        if !state.eof || state.idle {
            self.at_end = None;
        }
        if !passing {
            self.line = None;
        }
        let (whole, fade) = (self.shown, self.fade);
        let total = whole + fade;
        // The age read once: read again, it could be past the end.
        let age = self.line.as_ref().map(|(_, since)| since.elapsed());
        if age.is_some_and(|age| age >= total) {
            self.line = None;
        }
        let (Some((osd, _)), Some(age)) = (&mut self.line, age) else {
            if let Some(osd) = &self.at_end {
                paint_line(ctx, video, &osd.text(), look, scale, 1.0);
            }
            return;
        };
        if !state.seeking
            && let Some(t) = state.time
        {
            match osd {
                Osd::Seek { time, duration, .. } | Osd::Jump { time, duration } => {
                    *time = t;
                    *duration = state.duration;
                }
                _ => {}
            }
        }
        let alpha = if age > whole {
            total.saturating_sub(age).as_secs_f32() / fade.as_secs_f32().max(1e-3)
        } else {
            1.0
        };
        paint_line(ctx, video, &osd.text(), look, scale, alpha);
        // Woken for the fade, then every few frames through it.
        let next = if age < whole {
            whole - age
        } else {
            Duration::from_millis(30)
        };
        ctx.request_repaint_after(next);
    }
}

/// One line of the OSD over `video`, `alpha` of its colour.
fn paint_line(
    ctx: &egui::Context,
    video: Rect,
    text: &str,
    look: &OsdStyle,
    scale: f32,
    alpha: f32,
) {
    let size = (video.height() / 26.0).clamp(15.0, 30.0) * scale;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("osd"),
    ));
    let painter = painter.with_clip_rect(video);
    let pos = video.left_top() + vec2(size * 0.6, size * 0.45);
    paint_text(&painter, pos, text, size, look, alpha);
}

/// The OSD's `text` from `pos` (its top left corner) at `size` points in
/// `look`, `alpha` of its colours: also the settings' preview. The outline
/// is the letters drawn round them on two rings, the shadow the same
/// further down and right, in black.
pub fn paint_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    size: f32,
    look: &OsdStyle,
    alpha: f32,
) {
    let font = FontId::new(size, style::osd_family());
    let galley = painter.layout_no_wrap(text.to_owned(), font, Color32::WHITE);
    let draw = |offset: Vec2, color: Color32| {
        painter.galley_with_override_text_color(pos + offset, galley.clone(), color);
    };
    let rgb = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b).gamma_multiply(alpha);
    let unit = size / UNIT_SIZE;
    let outline = look.outline as f32 * unit;
    let ring = |radius: f32| -> Vec<Vec2> {
        if radius <= 0.0 {
            return Vec::new();
        }
        (0..16)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.0;
                vec2(a.cos(), a.sin()) * radius
            })
            .collect()
    };
    if look.boxed {
        let [r, g, b] = look.outline_color;
        let fill = Color32::from_rgba_unmultiplied(r, g, b, BOX_ALPHA).gamma_multiply(alpha);
        let text_rect = Rect::from_min_size(pos, galley.size());
        let margin = vec2(size * 0.25, size * 0.05) + Vec2::splat(outline);
        painter.rect_filled(text_rect.expand2(margin), size * 0.15, fill);
    } else {
        if look.shadow > 0.0 {
            let shadow = Color32::from_black_alpha(170).gamma_multiply(alpha);
            let offset = Vec2::splat(look.shadow as f32 * unit);
            for o in ring(outline) {
                draw(offset + o, shadow);
            }
            draw(offset, shadow);
        }
        for radius in [outline, outline / 2.0] {
            for o in ring(radius) {
                draw(o, rgb(look.outline_color));
            }
        }
    }
    let letters = rgb(look.color);
    draw(Vec2::ZERO, letters);
    if look.bold {
        draw(vec2(size * 0.035, 0.0), letters);
    }
}
