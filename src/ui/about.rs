//! The About dialog: icon, name, version, a short description, the author,
//! the homepage, the licence notice (the GPL v3's "Appropriate Legal
//! Notices": the program goes with libmpv) with the Licenses window, the
//! engine and where the settings are kept. The language and the update
//! check are in the settings only.

use egui::Key;

use super::licenses::LicensesView;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// Cargo joins several authors with `:`.
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
/// The home of mpv, whose library plays everything.
const MPV_URL: &str = "https://mpv.io";
/// Displayed icon size in points.
const ICON: f32 = 64.0;

#[derive(Default)]
pub struct AboutDialog {
    pub open: bool,
    /// Opened this frame: the key that opened it must not close it.
    pub fresh: bool,
    icon: super::IconCache,
    /// The Licenses window is shown in place of the dialog; closed, it
    /// gives way to the dialog again.
    licenses: Option<LicensesView>,
}

impl AboutDialog {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.fresh = self.open;
    }

    /// Draws the dialog while it is open; closes it on the cross, Esc,
    /// Enter or a click outside.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        if let Some(view) = &mut self.licenses {
            if view.show(ctx) {
                self.licenses = None;
                self.fresh = true;
            }
            return;
        }
        let fresh = std::mem::take(&mut self.fresh);
        let mut show_licenses = false;
        let icon = super::app_icon(ctx, ICON, &mut self.icon);
        let modal = egui::Modal::new(egui::Id::new("about")).show(ctx, |ui| {
            ui.set_width(340.0);
            // The cross in the top right corner, over the centred content
            // (a child Ui takes no room in the layout).
            let corner = egui::Rect::from_min_size(
                egui::pos2(ui.max_rect().right() - 24.0, ui.cursor().top()),
                egui::vec2(24.0, 24.0),
            );
            let closed =
                super::close_cross(&mut ui.new_child(egui::UiBuilder::new().max_rect(corner)));
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(ICON, ICON), egui::Sense::hover());
                super::paint_icon(ui.painter(), &icon, rect.center(), egui::Color32::WHITE);
                ui.add_space(6.0);
                ui.heading(crate::APP_NAME);
                ui.label(tr!(
                    format!("Version {}", crate::VERSION),
                    format!("Версия {}", crate::VERSION)
                ));
                ui.add_space(8.0);
                // One sentence, the same as Cargo.toml's description and
                // the README tagline; the Russian one is written by hand.
                ui.label(tr!(
                    "A fast video player for Windows built on libmpv.",
                    "Быстрый видеоплеер для Windows на основе libmpv."
                ));
                ui.add_space(8.0);
                if !AUTHORS.is_empty() {
                    let authors = AUTHORS.replace(':', ", ");
                    ui.label(tr!(
                        format!("Author: {authors}"),
                        format!("Автор: {authors}")
                    ));
                }
                if !REPOSITORY.is_empty()
                    && ui
                        .link(tr!("Homepage", "Сайт проекта"))
                        .on_hover_text(REPOSITORY)
                        .clicked()
                    && !crate::win::shell_open(REPOSITORY)
                {
                    log::warn!("could not open {REPOSITORY}");
                }
                ui.weak(tr!(
                    "Distributed with libmpv under the GNU GPL v3, without any warranty.",
                    "Распространяется вместе с libmpv на условиях GNU GPL v3, без каких-либо гарантий."
                ));
                if ui.link(tr!("Licenses…", "Лицензии…")).clicked() {
                    show_licenses = true;
                }
                // The engine: the client API of the DLL found, or why none was.
                let engine = match crate::mpv::sys::api() {
                    Ok(api) => {
                        let v = crate::mpv::sys::api_version(api);
                        tr!(
                            format!("Plays with libmpv (client API {v})"),
                            format!("Воспроизведение: libmpv (API {v})")
                        )
                    }
                    Err(_) => tr!("libmpv is not loaded".into(), "libmpv не загружена".into()),
                };
                if ui.link(engine).on_hover_text(MPV_URL).clicked()
                    && !crate::win::shell_open(MPV_URL)
                {
                    log::warn!("could not open {MPV_URL}");
                }
                let stored = crate::settings::location();
                let place = ui.weak(if stored.portable {
                    tr!(
                        "Settings: next to the program (portable)",
                        "Настройки: рядом с программой (портативно)"
                    )
                } else {
                    tr!(
                        "Settings: in the user profile",
                        "Настройки: в профиле пользователя"
                    )
                });
                match &stored.file {
                    Some(path) => place.on_hover_text(path.display().to_string()),
                    None => place
                        .on_hover_text(tr!("Settings are not saved", "Настройки не сохраняются")),
                };
                ui.add_space(4.0);
            });
            closed
        });
        let closed = modal.inner
            || (!fresh
                && (ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
                    || modal.should_close()));
        if show_licenses {
            self.licenses = Some(LicensesView::default());
        } else if closed {
            self.open = false;
        }
    }
}
