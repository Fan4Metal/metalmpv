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
}

impl OsdLine {
    pub fn show(&mut self, osd: Osd) {
        self.line = Some((osd, Instant::now()));
    }

    /// Paint the line over `video`. A seek's line shows where playback
    /// actually landed (a keyframe seek lands near, not on, the target)
    /// once mpv has finished the seek.
    pub fn paint(&mut self, ctx: &egui::Context, video: Rect, state: &State) {
        let Some((osd, since)) = &mut self.line else {
            return;
        };
        let age = since.elapsed();
        if age >= SHOWN {
            self.line = None;
            return;
        }
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
            (SHOWN - age).as_secs_f32() / FADE.as_secs_f32()
        } else {
            1.0
        };
        let size = (video.height() / 26.0).clamp(15.0, 30.0);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("osd"),
        ));
        let painter = painter.with_clip_rect(video);
        let pos = video.left_top() + vec2(size * 0.6, size * 0.45);
        let text = osd.text();
        let font = FontId::proportional(size);
        let shadow = egui::Color32::from_black_alpha((200.0 * alpha) as u8);
        for d in [
            vec2(1.5, 1.5),
            vec2(-1.0, 1.0),
            vec2(1.0, -1.0),
            vec2(-1.0, -1.0),
        ] {
            painter.text(pos + d, Align2::LEFT_TOP, &text, font.clone(), shadow);
        }
        painter.text(
            pos,
            Align2::LEFT_TOP,
            &text,
            font,
            style::OSD.gamma_multiply(alpha),
        );
        // Woken for the fade, then every few frames through it.
        let next = if age < SHOWN - FADE {
            SHOWN - FADE - age
        } else {
            Duration::from_millis(30)
        };
        ctx.request_repaint_after(next);
    }
}
