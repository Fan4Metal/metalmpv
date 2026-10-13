//! The Go to time window (`Ctrl+G`, the context menu): a field for a
//! time in the file (`format::parse_time`: `1:23:45`, `83:20`, seconds,
//! `50%`, `+30`, `-1:00`), shown as the position it names before it is
//! taken; the player goes there exactly.

use egui::{Key, RichText, TextEdit, vec2};

use super::style;
use crate::format::{clock_tenths_in, parse_time, percent_in};
use crate::i18n::lang;

#[derive(Default)]
pub struct GoToDialog {
    /// Shown, with the text typed so far.
    text: Option<String>,
    /// Opened this frame: the field takes the focus, its text selected.
    fresh: bool,
}

impl GoToDialog {
    /// Open the window with the position `now` in the field (selected,
    /// so typing replaces it).
    pub fn open(&mut self, now: Option<f64>, duration: Option<f64>) {
        let long = duration.is_some_and(|d| d >= 3600.0);
        self.text = Some(now.map_or_else(String::new, |t| crate::format::time(Some(t), long)));
        self.fresh = true;
    }

    pub fn is_open(&self) -> bool {
        self.text.is_some()
    }

    /// Draw the window while it is open; the time to go to once it is
    /// given (Enter or Go). Esc, Cancel or a click outside close it.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        now: Option<f64>,
        duration: Option<f64>,
    ) -> Option<f64> {
        let text = self.text.as_mut()?;
        let fresh = std::mem::take(&mut self.fresh);
        let mut chosen = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("go_to_time")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(
                RichText::new(tr!("Go to time", "Перейти ко времени"))
                    .size(17.0)
                    .color(style::TEXT_BRIGHT),
            );
            ui.add_space(8.0);
            let mut output = TextEdit::singleline(text)
                .hint_text("1:23:45")
                .desired_width(f32::INFINITY)
                .show(ui);
            if fresh {
                output.response.request_focus();
                let all = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                );
                output.state.cursor.set_char_range(Some(all));
                output.state.store(ui.ctx(), output.response.id);
            }
            let target = parse_time(text, now, duration);
            ui.add_space(4.0);
            let lang = lang();
            let (line, color) = match target {
                Some(t) => {
                    let share = duration
                        .filter(|d| *d > 0.0)
                        .map(|d| format!(" ({})", percent_in(lang, t / d)))
                        .unwrap_or_default();
                    (format!("→ {}{share}", clock_tenths_in(lang, t)), style::TEXT)
                }
                None if text.trim().is_empty() => (String::new(), style::TEXT_WEAK),
                None => (
                    tr!("Not a time in this file", "Это не время в этом файле").to_owned(),
                    style::OSD,
                ),
            };
            ui.label(RichText::new(line).color(color));
            ui.add_space(2.0);
            ui.label(
                RichText::new(tr!(
                    "1:23:45, 83:20 or 5000 (seconds); 50% of the file; +30 or -1:00 from here.",
                    "1:23:45, 83:20 или 5000 (секунды); 50% файла; +30 или -1:00 от текущей позиции."
                ))
                .size(12.0)
                .color(style::TEXT_WEAK),
            );
            ui.add_space(10.0);
            let entered =
                output.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                let go = ui.add_enabled(
                    target.is_some(),
                    egui::Button::new(tr!("Go", "Перейти")).min_size(vec2(90.0, 0.0)),
                );
                if go.clicked() || entered {
                    chosen = target;
                }
                if ui.button(tr!("Cancel", "Отмена")).clicked() {
                    close = true;
                }
            });
        });
        if chosen.is_some() || close || modal.should_close() {
            self.text = None;
        }
        chosen
    }
}
