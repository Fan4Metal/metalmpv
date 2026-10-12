//! The File Associations dialog, as qview's: whether MetalMPV is
//! registered with Windows, its types with their icons, extensions and
//! whether MetalMPV opens them by default, and the buttons that register,
//! open Windows' Default apps and remove the registration (`assoc.rs`).

use egui::{Align, Layout, TextureHandle};

use crate::assoc::{self, Opener, Status};
use crate::filetypes::FILE_TYPES;
use crate::i18n::Lang;

/// The side of a type's icon in points.
const ICON: f32 = 24.0;

#[derive(Default)]
pub struct AssocDialog {
    pub open: bool,
    /// Opened this frame: the key that opened it must not close it.
    fresh: bool,
    /// The types' icons for a pixel size, drawn on first show.
    icons: Option<(u32, Vec<TextureHandle>)>,
    /// What the last button did.
    message: Option<String>,
}

enum Action {
    Register,
    Choose,
    Unregister,
}

impl AssocDialog {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.fresh = self.open;
        self.message = None;
    }

    /// Draws the dialog while it is open; closes it on the cross, Esc or a
    /// click outside.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let fresh = std::mem::take(&mut self.fresh);
        let exe = std::env::current_exe().ok();
        // Read on every frame: cheap, and it follows choices made in
        // Settings while the dialog is open.
        let status = exe.as_deref().map_or(Status::NotRegistered, assoc::status);
        let defaults = assoc::defaults(exe.as_deref());
        let px = (ICON * ctx.pixels_per_point()).round() as u32;
        if self.icons.as_ref().is_none_or(|(n, _)| *n != px) {
            let icons = FILE_TYPES
                .iter()
                .map(|t| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [px as usize, px as usize],
                        &crate::type_icon::rgba(t, px),
                    );
                    ctx.load_texture(
                        format!("type_icon_{}_{px}", t.id),
                        image,
                        egui::TextureOptions::NEAREST,
                    )
                })
                .collect();
            self.icons = Some((px, icons));
        }
        let icons = self
            .icons
            .as_ref()
            .map(|(_, i)| i.clone())
            .unwrap_or_default();
        let ru = crate::i18n::lang() == Lang::Ru;

        let mut action = None;
        let modal = egui::Modal::new(egui::Id::new("associations")).show(ctx, |ui| {
            ui.set_width(560.0);
            let closed = ui
                .horizontal(|ui| {
                    ui.heading(tr!("File Associations", "Сопоставление файлов"));
                    ui.with_layout(Layout::right_to_left(Align::Center), super::close_cross)
                        .inner
                })
                .inner;
            ui.add_space(6.0);
            ui.label(match &status {
                Status::NotRegistered => tr!(
                    "MetalMPV is not registered with Windows yet.".to_string(),
                    "MetalMPV ещё не зарегистрирован в Windows.".to_string()
                ),
                Status::Registered => tr!(
                    "MetalMPV is registered for the types below.".to_string(),
                    "MetalMPV зарегистрирован для типов файлов ниже.".to_string()
                ),
                Status::OtherCopy(path) => tr!(
                    format!("Another copy of MetalMPV is registered: {path}"),
                    format!("Зарегистрирована другая копия MetalMPV: {path}")
                ),
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.5)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("types")
                        .num_columns(4)
                        .spacing([12.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("");
                            ui.weak(tr!("Type", "Тип"));
                            ui.weak(tr!("Extensions", "Расширения"));
                            ui.weak(tr!("Default", "По умолчанию"));
                            ui.end_row();
                            for (i, t) in FILE_TYPES.iter().enumerate() {
                                if let Some(icon) = icons.get(i) {
                                    ui.image((icon.id(), egui::vec2(ICON, ICON)));
                                }
                                ui.label(if ru { t.name_ru } else { t.name_en });
                                let extensions: Vec<String> =
                                    t.extensions.iter().map(|e| format!(".{e}")).collect();
                                ui.label(extensions.join(" "));
                                match defaults.get(i) {
                                    Some(Opener::Us) => ui.label("✔"),
                                    Some(Opener::UsElsewhere) => ui.label("✔").on_hover_text(tr!(
                                        "MetalMPV opens these files, as picked in \"Open with\". \
                                         Choosing MetalMPV for this type in Settings gives the \
                                         files its icon and name.",
                                        "Эти файлы открывает MetalMPV, выбранный через «Открыть \
                                         с помощью». Если выбрать MetalMPV для этого типа в \
                                         параметрах Windows, файлы получат его значок и название."
                                    )),
                                    _ => ui.label("—"),
                                };
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(8.0);
            ui.label(tr!(
                "Windows lets only the user choose the default program. After registering, \
                 \"Choose as Default\" opens Settings: there, MetalMPV and the types it should \
                 open are picked. Windows also offers MetalMPV the next time a video is opened.",
                "Программу по умолчанию Windows разрешает выбирать только пользователю. После \
                 регистрации кнопка «Выбрать по умолчанию» открывает параметры Windows: там \
                 выбираются MetalMPV и типы файлов, которые он будет открывать. Кроме того, \
                 Windows сама предложит MetalMPV при следующем открытии видео."
            ));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let register = match status {
                    Status::Registered => tr!("Update Registration", "Обновить регистрацию"),
                    _ => tr!("Register", "Зарегистрировать"),
                };
                if ui.button(register).clicked() {
                    action = Some(Action::Register);
                }
                let choose = egui::Button::new(tr!("Choose as Default…", "Выбрать по умолчанию…"));
                if ui
                    .add_enabled(status == Status::Registered, choose)
                    .clicked()
                {
                    action = Some(Action::Choose);
                }
                let remove = egui::Button::new(tr!("Unregister", "Удалить регистрацию"));
                if ui
                    .add_enabled(status != Status::NotRegistered, remove)
                    .clicked()
                {
                    action = Some(Action::Unregister);
                }
            });
            if let Some(message) = &self.message {
                ui.add_space(6.0);
                ui.weak(message);
            }
            closed
        });
        match action {
            Some(Action::Register) => {
                let result = exe
                    .ok_or_else(|| "the path of the program is unknown".to_string())
                    .and_then(|e| assoc::register(&e));
                self.message = Some(match result {
                    Ok(()) => tr!(
                        "MetalMPV is registered.".into(),
                        "MetalMPV зарегистрирован.".into()
                    ),
                    Err(e) => {
                        log::warn!("registration failed: {e}");
                        tr!(
                            format!("Registration failed: {e}"),
                            format!("Не удалось зарегистрировать: {e}")
                        )
                    }
                });
            }
            Some(Action::Choose) => {
                if !assoc::open_default_apps() {
                    self.message = Some(tr!(
                        "Cannot open Settings.".into(),
                        "Не удалось открыть параметры.".into()
                    ));
                }
            }
            Some(Action::Unregister) => {
                self.message = Some(match assoc::unregister() {
                    Ok(()) => tr!(
                        "The registration is removed.".into(),
                        "Регистрация удалена.".into()
                    ),
                    Err(e) => {
                        log::warn!("cannot unregister: {e}");
                        tr!(
                            format!("Cannot remove the registration: {e}"),
                            format!("Не удалось удалить регистрацию: {e}")
                        )
                    }
                });
            }
            None => {}
        }
        let closed = modal.inner || (!fresh && modal.should_close());
        if closed {
            self.open = false;
        }
    }
}
