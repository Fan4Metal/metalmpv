//! The File types page of the settings, after qview's associations
//! dialog: whether MetalMPV is registered with Windows, the buttons that
//! register, open Windows' Default apps and remove the registration
//! (`assoc.rs`), and the types with their icons, extensions (those
//! MetalMPV opens bright) and whether MetalMPV opens them by default.

use egui::{RichText, TextureHandle, Ui};

use super::settings::{divider, note, row, section};
use super::style;
use crate::assoc::{self, Opener, Status};
use crate::filetypes::FILE_TYPES;
use crate::i18n::Lang;

/// The side of a type's icon in points.
const ICON: f32 = 24.0;
/// The width of the registration's buttons.
const BUTTON: f32 = 150.0;

#[derive(Default)]
pub struct FileTypesPage {
    /// The registration and who opens each type, with when they were
    /// read: once a second (a hundred registry reads), and after a button.
    read: Option<(std::time::Instant, Status, Vec<Vec<Opener>>)>,
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
        // Read again every second: it follows choices made in Windows'
        // Settings while the page is open.
        if self
            .read
            .as_ref()
            .is_none_or(|(at, _, _)| at.elapsed() >= std::time::Duration::from_secs(1))
        {
            let status = exe.as_deref().map_or(Status::NotRegistered, assoc::status);
            self.read = Some((
                std::time::Instant::now(),
                status,
                assoc::defaults(exe.as_deref()),
            ));
        }
        let Some((_, status, defaults)) = self.read.clone() else {
            return;
        };
        let icons = self.icons(ui.ctx());
        let ru = crate::i18n::lang() == Lang::Ru;

        let mut action = None;
        section(ui, tr!("Registration", "Регистрация"), |ui| {
            // A row per action, the buttons of one width in a column.
            let button = |ui: &mut Ui, enabled: bool, text: &str, disabled_tip: Option<&str>| {
                let size = egui::vec2(BUTTON, ui.spacing().interact_size.y);
                let response = ui
                    .add_enabled_ui(enabled, |ui| ui.add_sized(size, egui::Button::new(text)))
                    .inner;
                match disabled_tip {
                    Some(tip) => response.on_disabled_hover_text(tip),
                    None => response,
                }
                .clicked()
            };
            let state = match &status {
                Status::NotRegistered => tr!(
                    "Not registered yet.".to_string(),
                    "Ещё не зарегистрирован.".to_string()
                ),
                Status::Registered => tr!(
                    "Registered for the types below.".to_string(),
                    "Зарегистрирован для типов файлов ниже.".to_string()
                ),
                Status::OtherCopy(path) => tr!(
                    format!("Another copy is registered: {path}"),
                    format!("Зарегистрирована другая копия: {path}")
                ),
            };
            let register = match status {
                Status::Registered => tr!("Update", "Обновить"),
                _ => tr!("Register", "Зарегистрировать"),
            };
            if row(
                ui,
                tr!("MetalMPV in Windows", "MetalMPV в Windows"),
                Some(&state),
                BUTTON,
                |ui| button(ui, true, register, None),
            ) {
                action = Some(Action::Register);
            }
            divider(ui);
            if row(
                ui,
                tr!("Default program", "Программа по умолчанию"),
                Some(tr!(
                    "Opens Default apps in Windows' Settings.",
                    "Открывает приложения по умолчанию в параметрах Windows."
                )),
                BUTTON,
                |ui| {
                    button(
                        ui,
                        status == Status::Registered,
                        tr!("Choose…", "Выбрать…"),
                        Some(tr!(
                            "Windows offers MetalMPV once it is registered.",
                            "Windows предложит MetalMPV после регистрации."
                        )),
                    )
                },
            ) {
                action = Some(Action::Choose);
            }
            divider(ui);
            if row(
                ui,
                tr!("Remove the registration", "Удаление регистрации"),
                Some(tr!(
                    "Everything the registration wrote is removed.",
                    "Удаляется всё, что записала регистрация."
                )),
                BUTTON,
                |ui| {
                    button(
                        ui,
                        status != Status::NotRegistered,
                        tr!("Unregister", "Удалить"),
                        None,
                    )
                },
            ) {
                action = Some(Action::Unregister);
            }
            if let Some(message) = &self.message {
                divider(ui);
                ui.add_space(8.0);
                ui.label(RichText::new(message).color(style::TEXT_WEAK));
                ui.add_space(8.0);
            }
        });
        note(
            ui,
            tr!(
                "Windows lets only the user choose the default program: MetalMPV and the types \
                 it should open are picked in Settings. Windows also offers MetalMPV the next \
                 time a video is opened.",
                "Программу по умолчанию Windows разрешает выбирать только пользователю: \
                 MetalMPV и типы файлов, которые он будет открывать, выбираются в параметрах \
                 Windows. Кроме того, Windows сама предложит MetalMPV при следующем открытии видео."
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
                        let openers = defaults.get(i).map(Vec::as_slice).unwrap_or_default();
                        extensions(ui, t.extensions, openers);
                        let ours = |o: &&Opener| **o != Opener::Other;
                        let count = openers.iter().filter(ours).count();
                        if count == 0 {
                            ui.label("—");
                        } else if count < t.extensions.len() {
                            ui.label(format!("{count}/{}", t.extensions.len()))
                                .on_hover_text(tr!(
                                    "MetalMPV opens the extensions in bright letters; the others \
                                     are chosen for it in Settings, each on its own.",
                                    "MetalMPV открывает расширения, выделенные ярким; остальные \
                                     выбираются для него в параметрах Windows по отдельности."
                                ));
                        } else if openers.contains(&Opener::UsElsewhere) {
                            ui.label("✔").on_hover_text(tr!(
                                "MetalMPV opens these files, as picked in \"Open with\". \
                                 Choosing MetalMPV for this type in Settings gives the files \
                                 its icon and name.",
                                "Эти файлы открывает MetalMPV, выбранный через «Открыть с \
                                 помощью». Если выбрать MetalMPV для этого типа в параметрах \
                                 Windows, файлы получат его значок и название."
                            ));
                        } else {
                            ui.label("✔");
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(6.0);
        });

        if action.is_some() {
            self.read = None;
        }
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

/// A type's extensions, those MetalMPV opens bright, the others faint,
/// each with who opens it in its tooltip.
fn extensions(ui: &mut Ui, extensions: &[&str], openers: &[Opener]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for (i, ext) in extensions.iter().enumerate() {
            let opener = openers.get(i).copied().unwrap_or(Opener::Other);
            let (color, tip) = match opener {
                Opener::Other => (
                    style::TEXT_WEAK,
                    tr!("Opened by another program", "Открывается другой программой"),
                ),
                _ => (
                    style::TEXT_BRIGHT,
                    tr!("Opened by MetalMPV", "Открывается MetalMPV"),
                ),
            };
            ui.label(RichText::new(format!(".{ext}")).color(color))
                .on_hover_text(tip);
        }
    });
}
