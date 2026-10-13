//! The settings window (F5 or the gear button): one surface without inner
//! frames, the categories in a darker column on the left with Reset and
//! Close under them, the page of the chosen category on the right in
//! cards of rows (the name and a hint on the left, the control on the
//! right). Unlike the family's other apps, the language and the update
//! check are here only, not in About.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect, Response, RichText, Sense,
    Stroke, Ui, pos2, vec2,
};

use super::style;
use crate::audio::{BOOSTS, Channels, Device, Passthrough, boost_label};
use crate::fonts::Family;
use crate::i18n::LangChoice;
use crate::settings::{
    MAX_STEP, MAX_TRANSPARENCY, OSD_SCALE_RANGE, OnTop, SUB_SCALE_RANGE, Settings,
};
use crate::update::{Status, Updates, release_url, version_of};

/// The size of the window's content in points.
const SIZE: egui::Vec2 = vec2(820.0, 560.0);
/// The width of the category column.
const SIDEBAR_WIDTH: f32 = 190.0;
/// The corner radius of the window.
const RADIUS: u8 = 10;

/// A category of the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Playback,
    Video,
    Audio,
    Text,
    Files,
    Updates,
    Keys,
}

impl Page {
    const ALL: [Page; 8] = [
        Page::General,
        Page::Playback,
        Page::Video,
        Page::Audio,
        Page::Text,
        Page::Files,
        Page::Updates,
        Page::Keys,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::General => tr!("General", "Общие"),
            Page::Playback => tr!("Playback", "Воспроизведение"),
            Page::Video => tr!("Video", "Видео"),
            Page::Audio => tr!("Sound", "Звук"),
            Page::Text => tr!("Subtitles and OSD", "Субтитры и OSD"),
            Page::Files => tr!("File types", "Типы файлов"),
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
                "Going on where a file was left; how far the arrow keys move playback; the chapter marks.",
                "Продолжение с места остановки; на сколько перематывают клавиши со стрелками; метки глав."
            ),
            Page::Video => tr!(
                "How video is decoded; the panels in fullscreen.",
                "Как декодируется видео; панели в полноэкранном режиме."
            ),
            Page::Audio => tr!(
                "Where the sound goes and with how many channels; the volume's boost and normalization.",
                "Куда выводится звук и сколько в нём каналов; усиление и нормализация громкости."
            ),
            Page::Text => tr!(
                "The font and the size of subtitles and of the on-screen messages.",
                "Шрифт и размер субтитров и экранных сообщений."
            ),
            Page::Files => tr!(
                "Registering MetalMPV with Windows for video and audio files.",
                "Регистрация MetalMPV в Windows для видео- и аудиофайлов."
            ),
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
    file_types: super::assoc::FileTypesPage,
}

/// What changed in the window this frame and has to be applied.
#[derive(Default)]
pub struct Changed {
    pub hwdec: bool,
    /// The subtitles' font or size.
    pub subs: bool,
    pub osd_font: bool,
    /// Forget where the files were left.
    pub forget_positions: bool,
    pub audio_channels: bool,
    pub volume_max: bool,
    pub normalize: bool,
    pub passthrough: bool,
}

impl SettingsDialog {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.fresh = self.open;
    }

    /// Draw the window while it is open, editing `settings` and `updates`
    /// in place (the language is applied at once); `hwdec_current` is the
    /// decoder mpv uses now, `audio_devices` the output devices there are.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        updates: &mut Updates,
        hwdec_current: Option<&str>,
        audio_devices: &[Device],
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
                    .corner_radius(RADIUS)
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                round_widgets(ui);
                // Every part gets a rectangle of its own with a top-down
                // layout: nested horizontal layouts would lay the groups
                // out side by side and widen the window past the screen.
                let (rect, _) = ui.allocate_exact_size(SIZE, Sense::hover());

                // The category column, flush with the window's left edge.
                let sidebar = Rect::from_min_size(rect.min, vec2(SIDEBAR_WIDTH, rect.height()));
                let inner = RADIUS - 1;
                ui.painter().rect_filled(
                    sidebar,
                    CornerRadius {
                        nw: inner,
                        sw: inner,
                        ne: 0,
                        se: 0,
                    },
                    style::SIDEBAR_BG,
                );
                ui.painter().text(
                    sidebar.left_top() + vec2(20.0, 20.0),
                    Align2::LEFT_TOP,
                    tr!("Settings", "Настройки"),
                    FontId::proportional(18.0),
                    style::TEXT_BRIGHT,
                );
                let list = Rect::from_min_max(
                    sidebar.left_top() + vec2(10.0, 62.0),
                    sidebar.right_bottom() - vec2(10.0, 100.0),
                );
                self.categories(ui, list);

                const BUTTON: f32 = 30.0;
                let close_rect = Rect::from_min_size(
                    pos2(sidebar.left() + 12.0, sidebar.bottom() - 14.0 - BUTTON),
                    vec2(SIDEBAR_WIDTH - 24.0, BUTTON),
                );
                let reset_rect = close_rect.translate(vec2(0.0, -BUTTON - 8.0));
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
                    reset(settings, updates, &mut changed);
                }
                close |= child(ui, close_rect)
                    .add_sized(
                        close_rect.size(),
                        egui::Button::new(tr!("Close", "Закрыть")),
                    )
                    .clicked();

                // The page.
                let page = Rect::from_min_max(pos2(sidebar.right(), rect.top()), rect.max);
                let cross = Rect::from_min_size(
                    pos2(page.right() - 14.0 - 24.0, page.top() + 14.0),
                    vec2(24.0, 24.0),
                );
                close |= super::close_cross(&mut child(ui, cross));
                // The scroll bar takes the right margin, next to the cards.
                let content = Rect::from_min_max(
                    page.left_top() + vec2(28.0, 18.0),
                    page.right_bottom() - vec2(10.0, 0.0),
                );
                let mut ui = child(ui, content);
                ui.label(
                    RichText::new(self.page.title())
                        .size(20.0)
                        .strong()
                        .color(style::TEXT_BRIGHT),
                );
                ui.add_space(2.0);
                ui.label(RichText::new(self.page.summary()).color(style::TEXT_WEAK));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(&mut ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(Margin {
                                left: 0,
                                right: 18,
                                top: 0,
                                bottom: 18,
                            })
                            .show(ui, |ui| match self.page {
                                Page::General => general(ui, settings),
                                Page::Updates => updates_page(ui, updates),
                                Page::Playback => playback(ui, settings, &mut changed),
                                Page::Video => changed.hwdec |= video(ui, settings, hwdec_current),
                                Page::Audio => {
                                    audio_page(ui, settings, audio_devices, &mut changed)
                                }
                                Page::Text => text(ui, settings, &mut changed),
                                Page::Files => self.file_types.show(ui),
                                Page::Keys => keys(ui),
                            });
                    });
            });
        if close || (!fresh && modal.should_close()) {
            self.open = false;
        }
        changed
    }

    /// The list of categories in `rect`.
    fn categories(&mut self, ui: &mut Ui, rect: Rect) {
        let mut y = rect.top();
        for page in Page::ALL {
            let row = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 34.0));
            y += 36.0;
            let response = ui.interact(
                row,
                ui.id().with(("settings_page", page as u8)),
                Sense::CLICK,
            );
            let selected = self.page == page;
            let fill = if selected {
                Color32::from_white_alpha(14)
            } else if response.hovered() {
                Color32::from_white_alpha(7)
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(row, 6.0, fill);
            if selected {
                // A short accent bar at the left edge marks the page.
                let bar =
                    Rect::from_center_size(pos2(row.left() + 3.0, row.center().y), vec2(3.0, 16.0));
                ui.painter().rect_filled(bar, 1.5, style::ACCENT);
            }
            let color = if selected {
                style::TEXT_BRIGHT
            } else {
                style::TEXT
            };
            ui.painter().text(
                pos2(row.left() + 16.0, row.center().y),
                Align2::LEFT_CENTER,
                page.title(),
                FontId::proportional(14.0),
                color,
            );
            if response.clicked() {
                self.page = page;
            }
        }
    }
}

/// Rounder buttons, lists and fields in the window.
fn round_widgets(ui: &mut Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    for v in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        v.corner_radius = CornerRadius::same(5);
    }
    ui.spacing_mut().button_padding = vec2(10.0, 4.0);
}

/// A part of the window in `rect`, laid out from the top.
fn child(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    )
}

/// A card: a lighter rounded ground without a frame.
fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(style::CARD_BG)
        .corner_radius(8.0)
        .inner_margin(Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A titled group of settings: the title over a card.
pub(super) fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.add_space(16.0);
    ui.label(RichText::new(title).strong().color(style::TEXT_BRIGHT));
    ui.add_space(4.0);
    card(ui, add);
}

/// A remark under a card.
pub(super) fn note(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).size(12.5).color(style::TEXT_WEAK));
}

/// A line between the rows of a card.
pub(super) fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, style::LINE),
    );
}

/// A row of a card: the name with an optional hint under it on the left,
/// what `control` adds right-aligned in `control_width` points on the
/// right, centred vertically.
pub(super) fn row<R>(
    ui: &mut Ui,
    title: &str,
    hint: Option<&str>,
    control_width: f32,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    const PAD: f32 = 8.0;
    let width = ui.available_width();
    let text_width = (width - control_width - 24.0).max(80.0);
    let title = ui.painter().layout(
        title.to_owned(),
        FontId::proportional(14.0),
        style::TEXT,
        text_width,
    );
    let hint = hint.map(|h| {
        ui.painter().layout(
            h.to_owned(),
            FontId::proportional(12.5),
            style::TEXT_WEAK,
            text_width,
        )
    });
    let text_height = title.size().y + hint.as_ref().map_or(0.0, |h| 2.0 + h.size().y);
    let height = text_height.max(ui.spacing().interact_size.y) + 2.0 * PAD;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mut y = rect.center().y - text_height / 2.0;
    let title_height = title.size().y;
    ui.painter()
        .galley(pos2(rect.left(), y), title, style::TEXT);
    y += title_height + 2.0;
    if let Some(hint) = hint {
        ui.painter()
            .galley(pos2(rect.left(), y), hint, style::TEXT_WEAK);
    }
    let control_rect = Rect::from_min_max(pos2(rect.right() - control_width, rect.top()), rect.max);
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    control(&mut ui)
}

/// An on/off switch.
fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(40.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, "")
    });
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(response.id, *on);
        let hovered = response.hovered();
        let radius = rect.height() / 2.0;
        let painter = ui.painter();
        if *on {
            let fill = if hovered {
                style::ACCENT_HOVER
            } else {
                style::ACCENT
            };
            painter.rect_filled(rect, radius, fill);
        } else {
            painter.rect_filled(rect, radius, style::TRACK);
        }
        let knob = if *on || hovered {
            style::TEXT_BRIGHT
        } else {
            style::TEXT_WEAK
        };
        let x = egui::lerp(rect.left() + radius..=rect.right() - radius, t);
        painter.circle_filled(pos2(x, rect.center().y), radius - 4.0 + t, knob);
    }
    response
}

fn general(ui: &mut Ui, settings: &mut Settings) {
    section(ui, tr!("Interface", "Интерфейс"), |ui| {
        row(
            ui,
            tr!("Language", "Язык"),
            Some(tr!(
                "\"System\" follows the language of Windows.",
                "«Системный» следует языку Windows."
            )),
            230.0,
            |ui| {
                let mut choice = settings.language;
                egui::ComboBox::from_id_salt("settings_language")
                    .selected_text(choice.label())
                    .width(220.0)
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
        divider(ui);
        row(
            ui,
            tr!("Pin the information window", "Закрепить окно сведений"),
            Some(tr!(
                "On, it stays at the player's top right corner; off, it is moved by its title and keeps its place.",
                "Включено — окно стоит в правом верхнем углу плеера; выключено — перетаскивается за заголовок и остаётся там, где его оставили."
            )),
            50.0,
            |ui| switch(ui, &mut settings.info_pinned),
        );
        divider(ui);
        row(
            ui,
            tr!("Snap to the screen's centre", "Притягивать к центру экрана"),
            Some(tr!(
                "While the window is moved, its centre is drawn to the screen's centre, across or up and down, when it comes within a few pixels, as in GOM Player; moved on, it comes away.",
                "При перетаскивании центр окна примагничивается к центру экрана по горизонтали или по вертикали, когда подходит к нему на несколько пикселей, как в GOM Player; если тянуть дальше, окно отрывается."
            )),
            50.0,
            |ui| switch(ui, &mut settings.snap_to_centre),
        );
        divider(ui);
        row(
            ui,
            tr!("Over the other windows", "Поверх других окон"),
            Some(tr!("Ctrl+T switches it.", "Ctrl+T переключает.")),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_on_top")
                    .selected_text(settings.on_top.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for o in OnTop::ALL {
                            ui.selectable_value(&mut settings.on_top, o, o.label());
                        }
                    });
            },
        );
    });
    section(
        ui,
        tr!("Playlist", "Список воспроизведения"),
        |ui| {
            row(
                ui,
                tr!("Add the folder's files", "Добавлять файлы папки"),
                Some(tr!(
                    "Opening a file adds the other media files of its folder.",
                    "При открытии файла добавляются остальные медиафайлы его папки."
                )),
                50.0,
                |ui| switch(ui, &mut settings.fill_playlist),
            );
        },
    );
}

fn playback(ui: &mut Ui, settings: &mut Settings, changed: &mut Changed) {
    section(
        ui,
        tr!("Resume", "Продолжение просмотра"),
        |ui| {
            row(
                ui,
                tr!(
                    "Go on from where it stopped",
                    "Продолжать с места остановки"
                ),
                Some(tr!(
                    "A file of 5 minutes or more opened again starts where it was left; Home goes to the beginning.",
                    "Файл длиной от 5 минут при повторном открытии начинается с того места, где остановился; Home переходит к началу."
                )),
                50.0,
                |ui| switch(ui, &mut settings.resume_playback),
            );
            divider(ui);
            row(
                ui,
                tr!("Remember the tracks", "Запоминать дорожки"),
                Some(tr!(
                    "A file opened again plays the audio track and the subtitles (or none) chosen in it last time.",
                    "Файл при повторном открытии воспроизводится с той звуковой дорожкой и теми субтитрами (или без них), что были выбраны в нём в прошлый раз."
                )),
                50.0,
                |ui| switch(ui, &mut settings.remember_tracks),
            );
            divider(ui);
            row(
                ui,
                tr!(
                    "Like tracks in new files",
                    "В новых файлах — похожие дорожки"
                ),
                Some(tr!(
                    "A file opened for the first time plays the audio track and the subtitles most like those last chosen by hand, by title and language: the same voice-over in the next episode of a series.",
                    "Файл, открытый впервые, воспроизводится со звуковой дорожкой и субтитрами, больше всего похожими на последние выбранные вручную, по названию и языку: та же озвучка в следующей серии сериала."
                )),
                50.0,
                |ui| switch(ui, &mut settings.prefer_last_tracks),
            );
            divider(ui);
            row(
                ui,
                tr!("Remember the recent files", "Запоминать недавние файлы"),
                Some(tr!(
                    "The start screen lists the files played last; off, none are kept and the list is cleared.",
                    "Стартовый экран показывает последние воспроизведённые файлы; выключено — они не запоминаются, а список очищается."
                )),
                50.0,
                |ui| switch(ui, &mut settings.remember_recent),
            );
            divider(ui);
            changed.forget_positions |= row(
                ui,
                tr!(
                    "Remembered positions and tracks",
                    "Запомненные позиции и дорожки"
                ),
                Some(tr!(
                    "Also the recent files of the start screen.",
                    "А также недавние файлы стартового экрана."
                )),
                160.0,
                |ui| ui.button(tr!("Forget all", "Забыть все")).clicked(),
            );
        },
    );
    section(ui, tr!("Seek steps", "Шаги перемотки"), |ui| {
        row(
            ui,
            tr!("Left / Right", "← / →"),
            Some(tr!(
                "The short step, on keyframes.",
                "Короткий шаг, по ключевым кадрам."
            )),
            120.0,
            |ui| step(ui, &mut settings.seek_short),
        );
        divider(ui);
        row(
            ui,
            tr!("Ctrl+Left / Right", "Ctrl+← / →"),
            Some(tr!(
                "The long step, on keyframes.",
                "Длинный шаг, по ключевым кадрам."
            )),
            120.0,
            |ui| step(ui, &mut settings.seek_long),
        );
    });
    note(
        ui,
        tr!(
            "A seek on keyframes is instant: playback goes on from the keyframe nearest to the target.",
            "Перемотка по ключевым кадрам мгновенна: воспроизведение продолжается с ключевого кадра, ближайшего к цели."
        ),
    );
    section(
        ui,
        tr!("Exact seeking", "Точная перемотка"),
        |ui| {
            row(
                ui,
                tr!("Shift+Left / Right", "Shift+← / →"),
                Some(tr!(
                    "Moves by 1 s to the exact frame.",
                    "Перематывает на 1 с точно до кадра."
                )),
                0.0,
                |_| (),
            );
            divider(ui);
            row(
                ui,
                ", / .",
                Some(tr!("Steps by one frame.", "Переходит на один кадр.")),
                0.0,
                |_| (),
            );
        },
    );
    section(ui, tr!("Chapters", "Главы"), |ui| {
        row(
            ui,
            tr!("Marks on the seek bar", "Метки на полосе перемотки"),
            Some(tr!(
                "Cuts in the bar where the chapters start; also in the context menu. The chapters themselves are listed under the playlist.",
                "Разрывы на полосе в начале глав; переключаются и в контекстном меню. Сами главы перечислены под списком воспроизведения."
            )),
            50.0,
            |ui| switch(ui, &mut settings.chapter_marks),
        );
    });
}

/// The video page; whether hardware decoding was switched.
fn video(ui: &mut Ui, settings: &mut Settings, hwdec_current: Option<&str>) -> bool {
    let mut changed = false;
    section(ui, tr!("Decoding", "Декодирование"), |ui| {
        changed = row(
            ui,
            tr!("Hardware decoding (GPU)", "Аппаратное декодирование (GPU)"),
            Some(tr!(
                "Decodes video on the graphics card where it can; off, the processor decodes everything. The change applies to the playing file at once.",
                "Видео декодирует видеокарта, где это возможно; если выключено, всё декодирует процессор. Изменение сразу применяется к воспроизводимому файлу."
            )),
            50.0,
            |ui| switch(ui, &mut settings.hwdec),
        )
        .changed();
        divider(ui);
        let now = match hwdec_current {
            Some(h) => h.to_owned(),
            None => tr!("software", "программное").to_owned(),
        };
        row(
            ui,
            tr!("In use now", "Сейчас используется"),
            None,
            200.0,
            |ui| ui.label(RichText::new(now).color(style::TEXT_BRIGHT)),
        );
    });
    section(
        ui,
        tr!("Fullscreen", "Полноэкранный режим"),
        |ui| {
            row(
                ui,
                tr!("File name at the top", "Название файла вверху"),
                Some(tr!(
                    "A strip with the name across the top of the screen, shown while the pointer is near the top edge.",
                    "Полоса с названием вдоль верхнего края экрана, появляется, когда указатель подведён к нему."
                )),
                50.0,
                |ui| switch(ui, &mut settings.fullscreen_title),
            );
            divider(ui);
            row(
                ui,
                tr!("Panel transparency", "Прозрачность панелей"),
                Some(tr!(
                    "The control bar and the file's name over the picture.",
                    "Панель управления и название файла поверх изображения."
                )),
                230.0,
                |ui| {
                    ui.spacing_mut().slider_width = 160.0;
                    ui.add(
                        egui::Slider::new(
                            &mut settings.fullscreen_transparency,
                            0.0..=MAX_TRANSPARENCY,
                        )
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .suffix(" %"),
                    );
                },
            );
        },
    );
    changed
}

/// The output device and channels, the volume's boost and normalization.
fn audio_page(ui: &mut Ui, settings: &mut Settings, devices: &[Device], changed: &mut Changed) {
    section(ui, tr!("Output", "Вывод"), |ui| {
        let chosen = &settings.audio_device;
        let present = chosen.is_empty() || devices.iter().any(|d| &d.name == chosen);
        let system = tr!("Windows' default", "Устройство Windows по умолчанию");
        let name_of = |name: &str| -> String {
            if name.is_empty() {
                return system.to_owned();
            }
            devices
                .iter()
                .find(|d| d.name == name)
                .map_or_else(|| name.to_owned(), |d| d.description.clone())
        };
        let hint = if present {
            tr!(
                "The sound moves to it at once. A device that is unplugged gives way to Windows' default until it is back.",
                "Звук переходит на него сразу. Пока выбранное устройство отключено, звук идёт через устройство Windows по умолчанию."
            )
        } else {
            tr!(
                "The chosen device is not connected: the sound goes to Windows' default until it is back.",
                "Выбранное устройство не подключено: звук идёт через устройство Windows по умолчанию, пока оно не вернётся."
            )
        };
        row(
            ui,
            tr!("Device", "Устройство"),
            Some(hint),
            290.0,
            |ui| {
                let mut choice = settings.audio_device.clone();
                egui::ComboBox::from_id_salt("settings_audio_device")
                    .selected_text(shorten(&name_of(&choice), 34))
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, String::new(), system);
                        for d in devices {
                            ui.selectable_value(&mut choice, d.name.clone(), &d.description);
                        }
                    });
                // The app gives mpv the device (`App::follow_audio_device`).
                settings.audio_device = choice;
            },
        );
        divider(ui);
        row(
            ui,
            tr!("Channels", "Каналы"),
            Some(tr!(
                "Auto: as the device is set up in Windows, more channels mixed down to it. A layout chosen by hand is what the sound is mixed to.",
                "«Авто» — как устройство настроено в Windows, лишние каналы сводятся под него. Раскладка, выбранная вручную, — то, под что сводится звук."
            )),
            160.0,
            |ui| {
                let mut choice = settings.audio_channels;
                egui::ComboBox::from_id_salt("settings_audio_channels")
                    .selected_text(choice.label())
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for c in Channels::ALL {
                            ui.selectable_value(&mut choice, c, c.label());
                        }
                    });
                if choice != settings.audio_channels {
                    settings.audio_channels = choice;
                    changed.audio_channels = true;
                }
            },
        );
        divider(ui);
        row(
            ui,
            tr!("To a receiver as it is", "На ресивер без декодирования"),
            Some(tr!(
                "Dolby and DTS sound goes out compressed and the receiver decodes it; the volume, normalization and channels here do not apply to it, and Windows gives the device to the player alone. A format the device does not take is decoded as usual.",
                "Звук Dolby и DTS уходит в сжатом виде, его декодирует ресивер; громкость, нормализация и каналы программы на него не действуют, а Windows отдаёт устройство только плееру. Формат, который устройство не принимает, декодируется как обычно."
            )),
            290.0,
            |ui| {
                let mut choice = settings.passthrough;
                egui::ComboBox::from_id_salt("settings_passthrough")
                    .selected_text(choice.label())
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for p in Passthrough::ALL {
                            ui.selectable_value(&mut choice, p, p.label());
                        }
                    });
                if choice != settings.passthrough {
                    settings.passthrough = choice;
                    changed.passthrough = true;
                }
            },
        );
    });
    section(ui, tr!("Volume", "Громкость"), |ui| {
        row(
            ui,
            tr!("Volume boost", "Усиление громкости"),
            Some(tr!(
                "Lets the volume go past 100%; a loud passage may then distort.",
                "Позволяет поднять громкость выше 100%; на громких местах возможны искажения."
            )),
            160.0,
            |ui| {
                let mut choice = settings.volume_max;
                egui::ComboBox::from_id_salt("settings_volume_max")
                    .selected_text(boost_label(choice))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for b in BOOSTS {
                            ui.selectable_value(&mut choice, b, boost_label(b));
                        }
                    });
                if choice != settings.volume_max {
                    settings.volume_max = choice;
                    changed.volume_max = true;
                }
            },
        );
        divider(ui);
        if row(
            ui,
            tr!("Normalization", "Нормализация"),
            Some(tr!(
                "Evens out the volume as the film plays: quiet dialogue louder, loud scenes quieter.",
                "Выравнивает громкость по ходу фильма: тихие диалоги громче, громкие сцены тише."
            )),
            50.0,
            |ui| switch(ui, &mut settings.normalize),
        )
        .changed()
        {
            changed.normalize = true;
        }
    });
    note(
        ui,
        tr!(
            "Ctrl+= and Ctrl+- move the sound against the picture by 100 ms, Ctrl+0 puts it back.",
            "Ctrl+= и Ctrl+- сдвигают звук относительно изображения на 100 мс, Ctrl+0 возвращает его на место."
        ),
    );
}

/// `text` cut to `chars` characters with an ellipsis.
fn shorten(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(chars - 1).collect();
        cut.push('…');
        cut
    }
}

/// The fonts and sizes of the subtitles and of the OSD line.
fn text(ui: &mut Ui, settings: &mut Settings, changed: &mut Changed) {
    section(ui, tr!("Subtitles", "Субтитры"), |ui| {
        let chosen = row(
            ui,
            tr!("Font", "Шрифт"),
            Some(tr!(
                "For text subtitles (SRT and the like); ASS subtitles keep their own fonts.",
                "Для текстовых субтитров (SRT и подобных); у субтитров ASS свои шрифты."
            )),
            230.0,
            |ui| font_choice(ui, "sub_font", &settings.sub_font),
        );
        if let Some(family) = chosen {
            settings.sub_font = family.map(|f| f.name.clone()).unwrap_or_default();
            changed.subs = true;
        }
        divider(ui);
        changed.subs |= row(
            ui,
            tr!("Size", "Размер"),
            Some(tr!(
                "Of all subtitles, ASS included.",
                "Всех субтитров, включая ASS."
            )),
            230.0,
            |ui| percent(ui, &mut settings.sub_scale, SUB_SCALE_RANGE),
        )
        .changed();
    });
    section(
        ui,
        tr!("On-screen messages", "Экранные сообщения"),
        |ui| {
            let chosen = row(
                ui,
                tr!("Font", "Шрифт"),
                Some(tr!(
                    "The yellow line in the top left corner after a seek, a volume change and the like.",
                    "Жёлтая строка в левом верхнем углу после перемотки, смены громкости и т. п."
                )),
                230.0,
                |ui| font_choice(ui, "osd_font", &settings.osd_font),
            );
            if let Some(family) = chosen {
                settings.osd_font = family.map(|f| f.name.clone()).unwrap_or_default();
                settings.osd_font_file = family.map(|f| (f.file.clone(), f.index));
                changed.osd_font = true;
            }
            divider(ui);
            row(
                ui,
                tr!("Size", "Размер"),
                Some(tr!(
                    "It also follows the height of the picture.",
                    "Также зависит от высоты изображения."
                )),
                230.0,
                |ui| percent(ui, &mut settings.osd_scale, OSD_SCALE_RANGE),
            );
        },
    );
}

/// A size in percent.
fn percent(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) -> Response {
    ui.spacing_mut().slider_width = 160.0;
    ui.add(
        egui::Slider::new(value, range)
            .step_by(5.0)
            .fixed_decimals(0)
            .suffix(" %"),
    )
}

/// A font family out of the system's, with a filter over the list; `current`
/// is the chosen one's name, empty for the default. What was picked this
/// frame: `Some(None)` is the default.
fn font_choice(ui: &mut Ui, id: &str, current: &str) -> Option<Option<&'static Family>> {
    let default = tr!("Default", "По умолчанию");
    let shown = if current.is_empty() { default } else { current };
    let filter_id = ui.id().with((id, "filter"));
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(220.0)
        .height(320.0)
        .show_ui(ui, |ui| {
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            let edit = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text(tr!("Search", "Поиск"))
                    .desired_width(f32::INFINITY),
            );
            if !edit.has_focus() && filter.is_empty() {
                edit.request_focus();
            }
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            if filter.is_empty() && ui.selectable_label(current.is_empty(), default).clicked() {
                picked = Some(None);
            }
            let Some(families) = crate::fonts::families(ui.ctx()) else {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.weak(tr!("Listing the fonts…", "Список шрифтов…"));
                });
                return;
            };
            let filter = filter.to_lowercase();
            for family in families
                .iter()
                .filter(|f| filter.is_empty() || f.name.to_lowercase().contains(&filter))
            {
                if ui
                    .selectable_label(family.name == current, &family.name)
                    .clicked()
                {
                    picked = Some(Some(family));
                }
            }
        });
    if picked.is_some() {
        ui.data_mut(|d| d.remove::<String>(filter_id));
    }
    picked
}

/// The update check.
fn updates_page(ui: &mut Ui, updates: &mut Updates) {
    section(
        ui,
        tr!("Update check", "Проверка обновлений"),
        |ui| {
            let enabled = row(
                ui,
                tr!("Check at start-up", "Проверять при запуске"),
                Some(tr!("Once a day.", "Раз в сутки.")),
                50.0,
                |ui| switch(ui, &mut updates.enabled),
            );
            if enabled.changed() && updates.enabled {
                updates.start_if_due(ui.ctx());
            }
            divider(ui);
            let installed = tr!(
                format!("Installed version: {}", crate::VERSION),
                format!("Установленная версия: {}", crate::VERSION)
            );
            let checking = updates.status == Status::Checking;
            let check = row(ui, &installed, None, 160.0, |ui| {
                ui.add_enabled(
                    !checking,
                    egui::Button::new(tr!("Check now", "Проверить сейчас")),
                )
                .clicked()
            });
            if check {
                updates.start(ui.ctx());
            }
            if updates.status != Status::Unknown {
                ui.add_space(2.0);
                update_status(ui, updates);
                ui.add_space(8.0);
            }
        },
    );
    section(
        ui,
        tr!("Privacy", "Конфиденциальность"),
        |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(tr!(
                "Only api.github.com is asked, with the program's version in the request; nothing is downloaded or installed. A newer version shows a button in the title row that opens its release page.",
                "Запрос уходит только к api.github.com и содержит лишь версию программы; ничего не скачивается и не устанавливается. О новой версии сообщает кнопка в строке заголовка, открывающая страницу выпуска."
            )).color(style::TEXT_WEAK));
            ui.add_space(8.0);
        },
    );
}

/// The outcome of the last check, if any.
fn update_status(ui: &mut Ui, updates: &Updates) {
    match &updates.status {
        Status::Unknown => {}
        Status::Checking => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.weak(tr!("Checking…", "Проверка…"));
            });
        }
        Status::UpToDate => {
            ui.weak(tr!(
                "This is the latest version",
                "Установлена последняя версия"
            ));
        }
        Status::Newer(tag) => {
            let version = version_of(tag);
            let url = release_url(tag);
            if ui
                .link(tr!(
                    format!("Version {version} is available"),
                    format!("Доступна версия {version}")
                ))
                .on_hover_text(url.as_str())
                .clicked()
                && !crate::win::shell_open(&url)
            {
                log::warn!("could not open {url}");
            }
        }
        Status::Failed(e) => {
            ui.weak(tr!(
                "Could not check for updates",
                "Не удалось проверить обновления"
            ))
            .on_hover_text(e);
        }
    }
}

/// The keys, as in the README.
fn keys(ui: &mut Ui) {
    let rows: [(&str, &str); 33] = [
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
        (
            "PgUp / PgDn",
            tr!("Previous / next file", "Предыдущий / следующий файл"),
        ),
        (
            tr!("Enter, F11, double click", "Enter, F11, двойной щелчок"),
            tr!("Fullscreen", "Полный экран"),
        ),
        (
            "1 / 2 / 3 / 4",
            tr!(
                "Window for 50% / 100% / 150% / 200% of the video",
                "Окно под 50% / 100% / 150% / 200% размера видео"
            ),
        ),
        (
            "5",
            tr!(
                "Largest window without black bars",
                "Развернуть без чёрных полос"
            ),
        ),
        (
            tr!("`", "Ё"),
            tr!("Window to the picture", "Окно по размеру картинки"),
        ),
        (
            "6",
            tr!(
                "Fullscreen without black bars (crops) / with them",
                "Полный экран без чёрных полос (с обрезкой) / с полосами"
            ),
        ),
        (
            "Esc",
            tr!(
                "Leave fullscreen; in a window, close the program",
                "Выйти из полноэкранного режима; в окне — закрыть программу"
            ),
        ),
        (
            "T",
            tr!("Audio and subtitles window", "Окно звука и субтитров"),
        ),
        ("A", tr!("Next audio track", "Следующая звуковая дорожка")),
        ("I", tr!("File information", "Сведения о файле")),
        (
            "F7",
            tr!(
                "Video and sound panel: colour, rotation, equalizer",
                "Панель видео и звука: цвет, поворот, эквалайзер"
            ),
        ),
        ("Ctrl+E", tr!("Save the frame", "Сохранить кадр")),
        (
            "O",
            tr!(
                "Position and chapter on the screen",
                "Позиция и глава на экране"
            ),
        ),
        (
            "S / J",
            tr!(
                "Next subtitles (then off)",
                "Следующие субтитры (затем выкл.)"
            ),
        ),
        ("V", tr!("Subtitles off / on", "Субтитры выкл. / вкл.")),
        (
            "Ctrl+T",
            tr!(
                "Over the other windows: never, always, while playing",
                "Поверх других окон: никогда, всегда, во время воспроизведения"
            ),
        ),
        (
            "R",
            tr!(
                "Repeat: off, the file, the list",
                "Повтор: выключен, файл, список"
            ),
        ),
        (
            "C / X / Z",
            tr!(
                "Faster / slower / normal speed",
                "Быстрее / медленнее / обычная скорость"
            ),
        ),
        (
            "Shift+. / Shift+, / Shift+0",
            tr!(
                "Subtitles later / earlier by 100 ms / in sync",
                "Субтитры позже / раньше на 100 мс / как было"
            ),
        ),
        (
            "Ctrl+= / Ctrl+- / Ctrl+0",
            tr!(
                "Sound later / earlier by 100 ms / in sync",
                "Звук позже / раньше на 100 мс / как было"
            ),
        ),
        ("Ctrl+O", tr!("Open files", "Открыть файлы")),
        (
            "Ctrl+U",
            tr!("Open an address (a stream)", "Открыть адрес (поток)"),
        ),
        (
            tr!("L, middle click", "L, средний щелчок"),
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
    ui.add_space(16.0);
    card(ui, |ui| {
        for (i, (key, action)) in rows.into_iter().enumerate() {
            if i > 0 {
                divider(ui);
            }
            key_row(ui, key, action);
        }
    });
}

/// A row of the key list: the key on a keycap ground, the action after it,
/// wrapped (by hand: a Grid sizes its columns to their content and would
/// not wrap a long action).
fn key_row(ui: &mut Ui, key: &str, action: &str) {
    const KEY_WIDTH: f32 = 210.0;
    const PAD: f32 = 6.0;
    let width = ui.available_width();
    let key = ui.painter().layout_no_wrap(
        key.to_owned(),
        FontId::proportional(13.0),
        style::TEXT_BRIGHT,
    );
    let action = ui.painter().layout(
        action.to_owned(),
        FontId::proportional(14.0),
        style::TEXT,
        width - KEY_WIDTH,
    );
    let cap = key.size() + vec2(16.0, 6.0);
    let height = cap.y.max(action.size().y) + 2.0 * PAD;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let cap = Rect::from_min_size(pos2(rect.left(), rect.center().y - cap.y / 2.0), cap);
    let painter = ui.painter();
    painter.rect_filled(cap, 5.0, style::KEY_BG);
    painter.galley(cap.min + vec2(8.0, 3.0), key, style::TEXT_BRIGHT);
    let y = rect.center().y - action.size().y / 2.0;
    painter.galley(pos2(rect.left() + KEY_WIDTH, y), action, style::TEXT);
}

/// A seek step in seconds.
fn step(ui: &mut Ui, value: &mut f64) {
    ui.spacing_mut().interact_size.x = 80.0;
    ui.add(
        egui::DragValue::new(value)
            .range(1.0..=MAX_STEP)
            .speed(0.5)
            .fixed_decimals(0)
            .suffix(tr!(" s", " с")),
    );
}

/// Put the settings of this window back to their defaults; what of it
/// has to be applied goes into `changed`.
fn reset(settings: &mut Settings, updates: &mut Updates, changed: &mut Changed) {
    let d = Settings::default();
    changed.hwdec |= settings.hwdec != d.hwdec;
    changed.subs |= settings.sub_font != d.sub_font || settings.sub_scale != d.sub_scale;
    changed.osd_font |= settings.osd_font != d.osd_font;
    if settings.language != d.language {
        crate::i18n::set_lang(d.language.resolve());
    }
    settings.language = d.language;
    settings.seek_short = d.seek_short;
    settings.seek_long = d.seek_long;
    settings.hwdec = d.hwdec;
    settings.fill_playlist = d.fill_playlist;
    settings.chapter_marks = d.chapter_marks;
    settings.resume_playback = d.resume_playback;
    settings.remember_tracks = d.remember_tracks;
    settings.prefer_last_tracks = d.prefer_last_tracks;
    settings.remember_recent = d.remember_recent;
    settings.fullscreen_transparency = d.fullscreen_transparency;
    settings.fullscreen_title = d.fullscreen_title;
    settings.info_pinned = d.info_pinned;
    settings.snap_to_centre = d.snap_to_centre;
    settings.on_top = d.on_top;
    changed.audio_channels |= settings.audio_channels != d.audio_channels;
    changed.volume_max |= settings.volume_max != d.volume_max;
    changed.normalize |= settings.normalize != d.normalize;
    changed.passthrough |= settings.passthrough != d.passthrough;
    settings.audio_device = d.audio_device;
    settings.audio_channels = d.audio_channels;
    settings.volume_max = d.volume_max;
    settings.normalize = d.normalize;
    settings.passthrough = d.passthrough;
    settings.sub_font = d.sub_font;
    settings.sub_scale = d.sub_scale;
    settings.osd_font = d.osd_font;
    settings.osd_font_file = d.osd_font_file;
    settings.osd_scale = d.osd_scale;
    updates.enabled = d.check_updates;
}
