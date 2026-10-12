//! The File types page of the settings, after qview's associations
//! dialog: whether MetalMPV is registered with Windows, the buttons that
//! register, open Windows' Default apps and remove the registration
//! (`assoc.rs`), and the types with their icons, extensions and whether
//! MetalMPV opens them by default.

use egui::{RichText, TextureHandle, Ui};

use super::settings::{note, section};
use super::style;
use crate::assoc::{self, Opener, Status};
use crate::filetypes::FILE_TYPES;
use crate::i18n::Lang;

/// The side of a type's icon in points.
const ICON: f32 = 24.0;

#[derive(Default)]
pub struct FileTypesPage {
    /// The types' icons for a pixel size, drawn when the page is first
    /// shown.
    icons: Option<(u32, Vec<TextureHandle>)>,
    /// What the last button did.
    message: Option<String>,
}

enum Action {
    Register,
    Choose,
    Unregister,
}

impl FileTypesPage {
    pub fn show(&mut self, ui: &mut Ui) {
        let exe = std::env::current_exe().ok();
        // Read on every frame: cheap, and it follows choices made in
        // Windows' Settings while the page is open.
        let status = exe.as_deref().map_or(Status::NotRegistered, assoc::status);
        let defaults = assoc::defaults(exe.as_deref());
        let icons = self.icons(ui.ctx());
        let ru = crate::i18n::lang() == Lang::Ru;

        let mut action = None;
        section(ui, tr!("Registration", "Регистрация"), |ui| {
            ui.add_space(8.0);
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
            ui.horizontal_wrapped(|ui| {
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
                ui.add_space(4.0);
                ui.label(RichText::new(message).color(style::TEXT_WEAK));
            }
            ui.add_space(8.0);
        });
        note(
            ui,
            tr!(
                "Windows lets only the user choose the default program. After registering, \
                 \"Choose as Default\" opens Settings: there, MetalMPV and the types it should \
                 open are picked. Windows also offers MetalMPV the next time a video is opened.",
                "Программу по умолчанию Windows разрешает выбирать только пользователю. После \
                 регистрации кнопка «Выбрать по умолчанию» открывает параметры Windows: там \
                 выбираются MetalMPV и типы файлов, которые он будет открывать. Кроме того, \
                 Windows сама предложит MetalMPV при следующем открытии видео."
            ),
        );

        section(ui, tr!("Types", "Типы"), |ui| {
            ui.add_space(6.0);
            egui::Grid::new("file_types")
                .num_columns(4)
                .spacing([12.0, 4.0])
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
                                 Choosing MetalMPV for this type in Settings gives the files \
                                 its icon and name.",
                                "Эти файлы открывает MetalMPV, выбранный через «Открыть с \
                                 помощью». Если выбрать MetalMPV для этого типа в параметрах \
                                 Windows, файлы получат его значок и название."
                            )),
                            _ => ui.label("—"),
                        };
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
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
                self.message = (!assoc::open_default_apps()).then(|| {
                    tr!(
                        "Cannot open Settings.".into(),
                        "Не удалось открыть параметры.".into()
                    )
                });
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
    }

    /// The types' icons at the pixel size of [`ICON`] points, drawn once
    /// per size (on the pixel grid: not scaled).
    fn icons(&mut self, ctx: &egui::Context) -> Vec<TextureHandle> {
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
        self.icons
            .as_ref()
            .map(|(_, i)| i.clone())
            .unwrap_or_default()
    }
}
