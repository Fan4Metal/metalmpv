//! The Open an address dialog (`Ctrl+U`): a field for a direct link to a
//! video or a stream (http, https, rtsp, …), which mpv plays like a file.
//! Pages of YouTube and other sites need yt-dlp, which the player does not
//! use (`ytdl=no`).

use egui::{Key, RichText, TextEdit, vec2};

use super::style;

#[derive(Default)]
pub struct UrlDialog {
    /// Shown, with the text typed so far.
    text: Option<String>,
    /// Opened this frame: the field takes the focus.
    fresh: bool,
}

impl UrlDialog {
    pub fn open(&mut self) {
        self.text = Some(String::new());
        self.fresh = true;
    }

    pub fn is_open(&self) -> bool {
        self.text.is_some()
    }

    /// Draw the dialog while it is open; the address to play once it is
    /// given (Enter or Open). Esc, Cancel or a click outside close it.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        let text = self.text.as_mut()?;
        let fresh = std::mem::take(&mut self.fresh);
        let mut chosen = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("open_url")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(
                RichText::new(tr!("Open an address", "Открыть адрес"))
                    .size(17.0)
                    .color(style::TEXT_BRIGHT),
            );
            ui.add_space(8.0);
            let field = ui.add(
                TextEdit::singleline(text)
                    .hint_text("https://…")
                    .desired_width(f32::INFINITY),
            );
            if fresh {
                field.request_focus();
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr!(
                    "A direct link to a video or a stream (http, https, rtsp, rtmp, udp…). Pages of YouTube and other sites are not supported.",
                    "Прямая ссылка на видео или поток (http, https, rtsp, rtmp, udp…). Страницы YouTube и других сайтов не поддерживаются."
                ))
                .size(12.0)
                .color(style::TEXT_WEAK),
            );
            ui.add_space(10.0);
            let address = text.trim().to_owned();
            let valid = crate::playlist::is_url(std::path::Path::new(&address));
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                let open = ui.add_enabled(
                    valid,
                    egui::Button::new(tr!("Open", "Открыть")).min_size(vec2(90.0, 0.0)),
                );
                if (open.clicked() || entered) && valid {
                    chosen = Some(address.clone());
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
