//! The on-screen display: a line of yellow text in the top left corner of
//! the picture after a seek, a volume change and the like, gone after a
//! moment ("[ Назад 10 сек / Ключевой кадр / 00:01:23 (12,5%) ]").

use std::time::{Duration, Instant};

use egui::{Align2, FontId, Rect, vec2};

use super::style;
use crate::format::Osd;
use crate::player::State;

/// How long a line stays, the fade at its end included.
const SHOWN: Duration = Duration::from_millis(2000);
const FADE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct OsdLine {
    line: Option<(Osd, Instant)>,
    /// Shown, without fading, while playback stays stopped at the end of
    /// the file (why it stopped: the data ran out); a passing line covers
    /// it for its while.
    at_end: Option<Osd>,
}

impl OsdLine {
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

    /// Paint the line over `video`, `scale` times its usual size (which
    /// follows the picture's height). A seek's line shows where playback
    /// actually landed (a keyframe seek lands near, not on, the target)
    /// once mpv has finished the seek.
    pub fn paint(&mut self, ctx: &egui::Context, video: Rect, state: &State, scale: f32) {
        if !state.eof || state.idle {
            self.at_end = None;
        }
        // The age read once: read again, it could be past SHOWN.
        let age = self.line.as_ref().map(|(_, since)| since.elapsed());
        if age.is_some_and(|age| age >= SHOWN) {
            self.line = None;
        }
        let (Some((osd, _)), Some(age)) = (&mut self.line, age) else {
            if let Some(osd) = &self.at_end {
                paint_line(ctx, video, &osd.text(), scale, 1.0);
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
        let alpha = if age > SHOWN - FADE {
            SHOWN.saturating_sub(age).as_secs_f32() / FADE.as_secs_f32()
        } else {
            1.0
        };
        paint_line(ctx, video, &osd.text(), scale, alpha);
        // Woken for the fade, then every few frames through it.
        let next = if age < SHOWN - FADE {
            SHOWN - FADE - age
        } else {
            Duration::from_millis(30)
        };
        ctx.request_repaint_after(next);
    }
}

/// One line of the OSD over `video`, `alpha` of its colour.
fn paint_line(ctx: &egui::Context, video: Rect, text: &str, scale: f32, alpha: f32) {
    let size = (video.height() / 26.0).clamp(15.0, 30.0) * scale;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("osd"),
    ));
    let painter = painter.with_clip_rect(video);
    let pos = video.left_top() + vec2(size * 0.6, size * 0.45);
    paint_text(&painter, pos, text, size, alpha);
}

/// The OSD's `text` from `pos` (its top left corner) at `size` points,
/// with its shadow: also the settings' preview.
pub fn paint_text(painter: &egui::Painter, pos: egui::Pos2, text: &str, size: f32, alpha: f32) {
    let font = FontId::new(size, style::osd_family());
    let shadow = egui::Color32::from_black_alpha((200.0 * alpha) as u8);
    for d in [
        vec2(1.5, 1.5),
        vec2(-1.0, 1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, -1.0),
    ] {
        painter.text(pos + d, Align2::LEFT_TOP, text, font.clone(), shadow);
    }
    painter.text(
        pos,
        Align2::LEFT_TOP,
        text,
        font,
        style::OSD.gamma_multiply(alpha),
    );
}
