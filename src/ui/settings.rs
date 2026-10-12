//! The settings window (F5 or the gear button): the categories in a list
//! on the left, Reset and Close under
//! it, the page of the chosen category on the right in framed groups. The
//! language and the update check are also in About, as in every app of
//! the family; both edit the same values.

use egui::{Margin, Rect, RichText, Stroke, Ui, pos2, vec2};

use super::style;
use crate::i18n::LangChoice;
use crate::settings::{MAX_STEP, Settings};
use crate::update::Updates;

/// The size of the window's content in points.
const SIZE: egui::Vec2 = vec2(820.0, 560.0);
/// The width of the category list.
const LIST_WIDTH: f32 = 170.0;

/// A category of the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Playback,
    Video,
    Updates,
    Keys,
}

impl Page {
    const ALL: [Page; 5] = [
        Page::General,
        Page::Playback,
        Page::Video,
        Page::Updates,
        Page::Keys,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::General => tr!("General", "Общие"),
            Page::Playback => tr!("Playback", "Воспроизведение"),
            Page::Video => tr!("Video", "Видео"),
            Page::Updates => tr!("Updates", "Обновления"),
            Page::Keys => tr!("Keys", "Клавиши"),
        }
    }

    /// The line under the page's title.
    fn summary(self) -> &'static str {
        match self {
            Page::General => tr!(
                "The interface language and how a playlist is made.",
                "Язык интерфейса и составление списка воспроизведения."
            ),
            Page::Playback => tr!(
                "How far the arrow keys move playback.",
                "На сколько перематывают клавиши со стрелками."
            ),
            Page::Video => tr!("How video is decoded.", "Как декодируется видео."),
            Page::Updates => tr!(
                "Asking GitHub whether a newer version is out.",
                "Проверка выхода новой версии на GitHub."
            ),
            Page::Keys => tr!(
                "The keys of the player (for reference; they cannot be changed).",
                "Клавиши плеера (для справки; изменить их нельзя)."
            ),
        }
    }
}

#[derive(Default)]
pub struct SettingsDialog {
    pub open: bool,
    /// Opened this frame: the key that opened it must not close it.
    fresh: bool,
    /// The category shown; kept while the program runs.
    page: Page,
}

/// What changed in the window this frame and has to be applied.
#[derive(Default)]
pub struct Changed {
    pub hwdec: bool,
}

impl SettingsDialog {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.fresh = self.open;
    }

    /// Draw the window while it is open, editing `settings` and `updates`
    /// in place (the language is applied at once); `hwdec_current` is the
    /// decoder mpv uses now.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        updates: &mut Updates,
        hwdec_current: Option<&str>,
    ) -> Changed {
        let mut changed = Changed::default();
        if !self.open {
            return changed;
        }
        let fresh = std::mem::take(&mut self.fresh);
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("settings"))
            .frame(
                egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                    .fill(style::PANEL_BG)
                    .inner_margin(Margin::same(10)),
            )
            .show(ctx, |ui| {
                // Every part gets a rectangle of its own with a top-down
                // layout: nested horizontal layouts would lay the groups
                // out side by side and widen the window past the screen.
                let (rect, _) = ui.allocate_exact_size(SIZE, egui::Sense::hover());

                let title = Rect::from_min_size(rect.min, vec2(rect.width(), 28.0));
                child(ui, title).heading(tr!("Settings", "Настройки"));
                let cross =
                    Rect::from_min_size(pos2(title.right() - 24.0, title.top()), vec2(24.0, 24.0));
                close |= super::close_cross(&mut child(ui, cross));

                let body = Rect::from_min_max(pos2(rect.left(), title.bottom() + 8.0), rect.max);
                const BUTTON: f32 = 28.0;
                let list = Rect::from_min_size(
                    body.min,
                    vec2(LIST_WIDTH, body.height() - 2.0 * BUTTON - 16.0),
                );
                self.categories(&mut child(ui, list));
                let reset_rect = Rect::from_min_size(
                    pos2(body.left(), list.bottom() + 8.0),
                    vec2(LIST_WIDTH, BUTTON),
                );
                let close_rect = Rect::from_min_size(
                    pos2(body.left(), reset_rect.bottom() + 8.0),
                    vec2(LIST_WIDTH, BUTTON),
                );
                if child(ui, reset_rect)
                    .add_sized(
                        reset_rect.size(),
                        egui::Button::new(tr!("Reset", "Сбросить")),
                    )
                    .on_hover_text(tr!(
                        "Put every setting back to its default",
                        "Вернуть всем настройкам значения по умолчанию"
                    ))
                    .clicked()
                {
                    changed.hwdec |= reset(settings, updates);
                }
                close |= child(ui, close_rect)
                    .add_sized(
                        close_rect.size(),
                        egui::Button::new(tr!("Close", "Закрыть")),
                    )
                    .clicked();

                let page = Rect::from_min_max(pos2(list.right() + 10.0, body.top()), body.max);
                ui.painter().rect_filled(page, 4.0, style::LIST_BG);
                let mut ui = child(ui, page.shrink(12.0));
                ui.label(RichText::new(self.page.title()).size(17.0).strong());
                ui.weak(self.page.summary());
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(&mut ui, |ui| match self.page {
                        Page::General => general(ui, settings),
                        Page::Updates => updates_page(ui, updates),
                        Page::Playback => playback(ui, settings),
                        Page::Video => changed.hwdec |= video(ui, settings, hwdec_current),
                        Page::Keys => keys(ui),
                    });
            });
        if close || (!fresh && modal.should_close()) {
            self.open = false;
        }
        changed
    }

    /// The list of categories, filling `ui`.
    fn categories(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 4.0, style::LIST_BG);
        let mut y = rect.top() + 6.0;
        for page in Page::ALL {
            let row =
                Rect::from_min_size(pos2(rect.left() + 6.0, y), vec2(rect.width() - 12.0, 30.0));
            y += 32.0;
            let response = ui.interact(
                row,
                ui.id().with(("settings_page", page as u8)),
                egui::Sense::CLICK,
            );
            let selected = self.page == page;
            let fill = if selected {
                style::SELECTED_BG
            } else if response.hovered() {
                style::HOVER
            } else {
                style::LIST_BG
            };
            ui.painter().rect_filled(row, 3.0, fill);
            if selected {
                ui.painter().rect_stroke(
                    row,
                    3.0,
                    Stroke::new(1.0, style::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            let color = if selected {
                style::TEXT_BRIGHT
            } else {
                style::TEXT
            };
            ui.painter().text(
                row.center(),
                egui::Align2::CENTER_CENTER,
                page.title(),
                egui::FontId::proportional(14.0),
                color,
            );
            if response.clicked() {
                self.page = page;
            }
        }
    }
}

/// A part of the window in `rect`, laid out from the top.
fn child(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    )
}

/// A framed group with a title.
fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.add_space(8.0);
    egui::Frame::new()
        .stroke(Stroke::new(1.0, style::LINE))
        .corner_radius(4.0)
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong());
            ui.add_space(4.0);
            add(ui);
        });
}

fn general(ui: &mut Ui, settings: &mut Settings) {
    group(
        ui,
        tr!("Interface language", "Язык интерфейса"),
        |ui| {
            let mut choice = settings.language;
            egui::ComboBox::from_id_salt("settings_language")
                .selected_text(choice.label())
                .width(240.0)
                .show_ui(ui, |ui| {
                    for c in LangChoice::ALL {
                        ui.selectable_value(&mut choice, c, c.label());
                    }
                });
            if choice != settings.language {
                settings.language = choice;
                crate::i18n::set_lang(choice.resolve());
            }
        },
    );
    group(
        ui,
        tr!("Playlist", "Список воспроизведения"),
        |ui| {
            ui.checkbox(
                &mut settings.fill_playlist,
                tr!(
                    "Opening a file adds the other media files of its folder",
                    "При открытии файла добавлять остальные медиафайлы его папки"
                ),
            );
        },
    );
}

fn playback(ui: &mut Ui, settings: &mut Settings) {
    group(ui, tr!("Seek steps", "Шаги перемотки"), |ui| {
        ui.weak(tr!(
            "A seek on keyframes is instant: playback goes on from the keyframe nearest to the target.",
            "Перемотка по ключевым кадрам мгновенна: воспроизведение продолжается с ключевого кадра, ближайшего к цели."
        ));
        ui.add_space(6.0);
        egui::Grid::new("seek_steps")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr!("Left / Right:", "← / →:"));
                step(ui, &mut settings.seek_short);
                ui.end_row();
                ui.label(tr!("Ctrl+Left / Right:", "Ctrl+← / →:"));
                step(ui, &mut settings.seek_long);
                ui.end_row();
            });
    });
    group(
        ui,
        tr!("Exact seeking", "Точная перемотка"),
        |ui| {
            ui.label(tr!(
                "Shift+Left / Right moves by 1 s to the exact frame; , and . step by one frame.",
                "Shift+← / → перематывает на 1 с точно до кадра; «,» и «.» — на один кадр."
            ));
        },
    );
}

/// The video page; whether hardware decoding was switched.
fn video(ui: &mut Ui, settings: &mut Settings, hwdec_current: Option<&str>) -> bool {
    let mut changed = false;
    group(ui, tr!("Decoding", "Декодирование"), |ui| {
        changed = ui
            .checkbox(&mut settings.hwdec, tr!("Hardware decoding (GPU)", "Аппаратное декодирование (GPU)"))
            .on_hover_text(tr!(
                "Decodes video on the graphics card where it can; off, the processor decodes everything",
                "Видео декодирует видеокарта, где это возможно; если выключено, всё декодирует процессор"
            ))
            .changed();
        ui.add_space(4.0);
        let now = match hwdec_current {
            Some(h) => tr!(
                format!("In use now: {h}"),
                format!("Сейчас используется: {h}")
            ),
            None => tr!(
                "In use now: software".into(),
                "Сейчас используется: программное".into()
            ),
        };
        ui.weak(now);
        ui.weak(tr!(
            "The change applies to the playing file at once.",
            "Изменение сразу применяется к воспроизводимому файлу."
        ));
    });
    changed
}

/// The update check, the same as in About.
fn updates_page(ui: &mut Ui, updates: &mut Updates) {
    group(
        ui,
        tr!("Update check", "Проверка обновлений"),
        |ui| {
            ui.label(tr!(
                format!("Installed version: {}", crate::VERSION),
                format!("Установленная версия: {}", crate::VERSION)
            ));
            ui.add_space(4.0);
            super::about::update_section(ui, updates, false);
        },
    );
    group(
        ui,
        tr!("Privacy", "Конфиденциальность"),
        |ui| {
            ui.weak(tr!(
            "Only api.github.com is asked, with the program's version in the request; nothing is downloaded or installed. A newer version shows a button in the title row that opens its release page.",
            "Запрос уходит только к api.github.com и содержит лишь версию программы; ничего не скачивается и не устанавливается. О новой версии сообщает кнопка в строке заголовка, открывающая страницу выпуска."
        ));
        },
    );
}

/// The keys, as in the README.
fn keys(ui: &mut Ui) {
    let rows: [(&str, &str); 16] = [
        (
            tr!("Space", "Пробел"),
            tr!("Play / pause", "Воспроизведение / пауза"),
        ),
        (
            "← / →",
            tr!(
                "Seek by the short step, keyframes",
                "Перемотка на короткий шаг, по ключевым кадрам"
            ),
        ),
        (
            "Ctrl+← / →",
            tr!(
                "Seek by the long step, keyframes",
                "Перемотка на длинный шаг, по ключевым кадрам"
            ),
        ),
        (
            "Shift+← / →",
            tr!("Seek by 1 s, exact", "Перемотка на 1 с, точно"),
        ),
        (
            ", / .  (D / F)",
            tr!("Previous / next frame", "Предыдущий / следующий кадр"),
        ),
        (
            "Home / End",
            tr!("Start / end of the file", "Начало / конец файла"),
        ),
        (
            tr!("↑ / ↓, wheel", "↑ / ↓, колесо"),
            tr!("Volume", "Громкость"),
        ),
        ("M", tr!("Mute", "Без звука")),
        ("S", tr!("Stop", "Стоп")),
        (
            "PgUp / PgDn",
            tr!("Previous / next file", "Предыдущий / следующий файл"),
        ),
        (
            tr!("Enter, F11, double click", "Enter, F11, двойной щелчок"),
            tr!("Fullscreen", "Полный экран"),
        ),
        (
            "Esc",
            tr!("Leave fullscreen", "Выйти из полноэкранного режима"),
        ),
        ("Ctrl+O", tr!("Open files", "Открыть файлы")),
        (
            "L",
            tr!("Show or hide the playlist", "Показать или скрыть список"),
        ),
        (
            "Delete",
            tr!(
                "Remove the selected playlist entry",
                "Убрать выбранный файл из списка"
            ),
        ),
        (
            "F5 / F1",
            tr!("Settings / About", "Настройки / О программе"),
        ),
    ];
    ui.add_space(8.0);
    // Two columns by hand: a Grid sizes its columns to their content and
    // would not wrap a long action.
    const KEY_WIDTH: f32 = 190.0;
    for (i, (key, action)) in rows.into_iter().enumerate() {
        let width = ui.available_width();
        let row = ui.horizontal_top(|ui| {
            ui.add_sized(
                vec2(KEY_WIDTH, 22.0),
                egui::Label::new(RichText::new(key).strong().color(style::TEXT_BRIGHT))
                    .halign(egui::Align::Min),
            );
            ui.add(egui::Label::new(action).wrap());
        });
        if i % 2 == 1 {
            let r = row.response.rect;
            let stripe = Rect::from_min_size(r.min, vec2(width, r.height()));
            ui.painter().rect_filled(
                stripe.expand2(vec2(0.0, 2.0)),
                2.0,
                egui::Color32::from_white_alpha(6),
            );
        }
    }
}

/// A seek step in seconds.
fn step(ui: &mut Ui, value: &mut f64) {
    ui.add(
        egui::DragValue::new(value)
            .range(1.0..=MAX_STEP)
            .speed(0.5)
            .fixed_decimals(0)
            .suffix(tr!(" s", " с")),
    );
}

/// Put the settings of this window back to their defaults; whether
/// hardware decoding changed.
fn reset(settings: &mut Settings, updates: &mut Updates) -> bool {
    let d = Settings::default();
    let hwdec = settings.hwdec != d.hwdec;
    if settings.language != d.language {
        crate::i18n::set_lang(d.language.resolve());
    }
    settings.language = d.language;
    settings.seek_short = d.seek_short;
    settings.seek_long = d.seek_long;
    settings.hwdec = d.hwdec;
    settings.fill_playlist = d.fill_playlist;
    updates.enabled = d.check_updates;
    hwdec
}
