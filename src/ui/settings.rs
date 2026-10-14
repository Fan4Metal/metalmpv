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

use super::style::{self, Accent};
use crate::audio::Device;
use crate::fonts::Family;
use crate::i18n::LangChoice;
use crate::input::Cmd;
use crate::keymap::{Action, Chord, Group, Keymap, shortcut};
use crate::settings::{
    Interface, MAX_STEP, MAX_TRANSPARENCY, MIN_STEP, MediaKeys, OSD_SCALE_RANGE, OnTop, Settings,
    WindowOnOpen,
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
    Tracks,
    Video,
    Files,
    Keys,
    Updates,
}

impl Page {
    const ALL: [Page; 7] = [
        Page::General,
        Page::Playback,
        Page::Tracks,
        Page::Video,
        Page::Files,
        Page::Keys,
        Page::Updates,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::General => tr!("General", "Общие"),
            Page::Playback => tr!("Playback", "Воспроизведение"),
            Page::Tracks => tr!("Tracks", "Дорожки"),
            Page::Video => tr!("Video", "Видео"),
            Page::Files => tr!("File types", "Типы файлов"),
            Page::Updates => tr!("Updates", "Обновления"),
            Page::Keys => tr!("Keyboard shortcuts", "Горячие клавиши"),
        }
    }

    /// The line under the page's title.
    fn summary(self) -> &'static str {
        match self {
            Page::General => tr!(
                "The language, the colour and layout of the interface, the window, how a playlist is made and the on-screen messages. The sound and the subtitles' look are on the Video, sound and subtitles panel.",
                "Язык, цвет и вид интерфейса, окно, составление плейлиста и экранные сообщения. Звук и вид субтитров — на панели «Видео, звук и субтитры»."
            ),
            Page::Playback => tr!(
                "What is remembered of the files played; how far the arrow keys move playback; the chapter marks; files not fully downloaded; the media keys.",
                "Что запоминается о воспроизведённых файлах; на сколько перематывают клавиши со стрелками; метки глав; неполные файлы; медиаклавиши."
            ),
            Page::Tracks => tr!(
                "The sound and the subtitles a file starts with, and the subtitle files loaded with it.",
                "С каким звуком и субтитрами начинается файл и какие файлы субтитров подхватываются."
            ),
            Page::Video => tr!(
                "How video is decoded; the panels in fullscreen.",
                "Как декодируется видео; панели в полноэкранном режиме."
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
                "The keys of the player and what they do.",
                "Клавиши плеера и что они делают."
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
    /// A key being chosen on the Keys page: the action, and which of its
    /// keys it replaces (`None`: one more).
    capture: Option<(Action, Option<usize>)>,
    /// The press that gave the choice up: its click must not start it
    /// again.
    gave_up: Option<(Action, Option<usize>)>,
    /// The last key chosen, the action that lost it and the keys before,
    /// for Undo.
    taken: Option<(Chord, Action, Keymap)>,
    /// What the Keys page shows: actions whose name or keys have it.
    key_filter: String,
}

/// What changed in the window this frame and has to be applied.
#[derive(Default)]
pub struct Changed {
    pub hwdec: bool,
    pub osd_font: bool,
    /// Forget where the files were left.
    pub forget_positions: bool,
    /// The keys (`keymap::publish`).
    pub keys: bool,
    /// The accent colour (`style::set_accent`, the window icon).
    pub accent: bool,
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
        if self.page != Page::Keys {
            self.capture = None;
        }
        self.capture_key(ctx, &mut settings.keys, &mut changed);
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
                // Smaller in a small window, so Close and the cross stay
                // on the screen.
                let size = super::modal_size(ctx, SIZE, 12.0);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

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
                        "Put the settings of this window back to their defaults; the panel's are reset on the panel",
                        "Вернуть настройкам этого окна значения по умолчанию; настройки панели сбрасываются на панели"
                    ))
                    .clicked()
                {
                    reset(settings, updates, &mut changed);
                    self.capture = None;
                    self.taken = None;
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
                                Page::General => general(ui, settings, &mut changed),
                                Page::Updates => updates_page(ui, updates),
                                Page::Playback => playback(ui, settings, &mut changed),
                                Page::Tracks => tracks_page(ui, settings),
                                Page::Video => changed.hwdec |= video(ui, settings, hwdec_current),
                                Page::Files => self.file_types.show(ui),
                                Page::Keys => self.keys_page(ui, &mut settings.keys, &mut changed),
                            });
                    });
            });
        // A click that gave up waiting for a key (on the backdrop too)
        // does not close the window.
        if close || (!fresh && self.gave_up.is_none() && modal.should_close()) {
            self.open = false;
            self.capture = None;
            self.taken = None;
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.gave_up = None;
        }
        changed
    }

    /// The list of categories in `rect`; it scrolls when the window is
    /// too low for it.
    fn categories(&mut self, ui: &mut Ui, rect: Rect) {
        let mut list = child(ui, rect);
        egui::ScrollArea::vertical()
            .id_salt("settings_pages")
            .auto_shrink([false, false])
            .show(&mut list, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for page in Page::ALL {
                    self.category(ui, page);
                }
            });
    }

    /// One row of the categories.
    fn category(&mut self, ui: &mut Ui, page: Page) {
        let (row, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::CLICK);
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
            ui.painter().rect_filled(bar, 1.5, style::accent());
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

/// The width of the accent colours' row.
const ACCENTS_WIDTH: f32 = 250.0;
/// The size of an accent's swatch.
const SWATCH: f32 = 22.0;

/// The accent colours as round swatches, the chosen one ringed, and last
/// one's own colour, picked in a popup; whether one was chosen.
fn accents(ui: &mut Ui, accent: &mut Accent) -> bool {
    let mut chosen = false;
    ui.spacing_mut().item_spacing.x = 5.0;
    // Right to left: the own colour first, then the ready ones from the last.
    chosen |= own_accent(ui, accent);
    for a in Accent::ALL.into_iter().rev() {
        let (rect, response) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
        let response = response.on_hover_text(a.label());
        let selected = *accent == a;
        swatch(ui, rect, selected, response.hovered(), |p, centre, r| {
            p.circle_filled(centre, r, a.palette().accent);
        });
        if response.clicked() && !selected {
            *accent = a;
            chosen = true;
        }
    }
    chosen
}

/// A round swatch in `rect`: ringed when chosen, a little bigger under the
/// pointer; `fill` paints the inside at a centre and a radius.
fn swatch(
    ui: &Ui,
    rect: Rect,
    selected: bool,
    hovered: bool,
    fill: impl FnOnce(&egui::Painter, egui::Pos2, f32),
) {
    let painter = ui.painter();
    let centre = rect.center();
    let radius = SWATCH / 2.0;
    if selected {
        painter.circle_stroke(centre, radius - 1.0, Stroke::new(2.0, style::TEXT_BRIGHT));
        fill(painter, centre, radius - 4.5);
    } else {
        let grow = if hovered { 1.0 } else { 0.0 };
        fill(painter, centre, radius - 3.0 + grow);
    }
}

/// The own colour's swatch: a colour wheel, or the colour once chosen; a
/// click opens the picker with a field for `#RRGGBB`, which starts from
/// the accent in use.
fn own_accent(ui: &mut Ui, accent: &mut Accent) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
    let response = response.on_hover_text(tr!("Your own colour…", "Свой цвет…"));
    let own = matches!(accent, Accent::Custom(_));
    let current = accent.palette().accent;
    swatch(ui, rect, own, response.hovered(), |p, centre, r| {
        if own {
            p.circle_filled(centre, r, current);
        } else {
            colour_wheel(p, centre, r);
        }
    });
    let mut chosen = false;
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut colour = accent.palette().accent;
            if egui::color_picker::color_picker_color32(
                ui,
                &mut colour,
                egui::color_picker::Alpha::Opaque,
            ) {
                *accent = custom(colour.to_array()[..3].try_into().expect("three"));
                chosen = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tr!("Hex", "Код"));
                // Kept as typed while the field has the focus.
                let id = ui.id().with("accent_hex");
                let mut text = ui
                    .data_mut(|d| d.get_temp::<String>(id))
                    .unwrap_or_else(|| format!("#{:06X}", accent.base()));
                // No limit: a pasted colour may come with spaces.
                let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(80.0));
                if field.changed()
                    && let Some(typed) = Accent::typed(&text)
                {
                    *accent = typed;
                    chosen = true;
                }
                if field.has_focus() {
                    ui.data_mut(|d| d.insert_temp(id, text));
                } else {
                    ui.data_mut(|d| d.remove::<String>(id));
                }
            });
        });
    chosen
}

fn custom([r, g, b]: [u8; 3]) -> Accent {
    Accent::Custom(u32::from_be_bytes([0, r, g, b]))
}

/// A disc of all the hues round its centre.
fn colour_wheel(painter: &egui::Painter, centre: egui::Pos2, radius: f32) {
    const SLICES: usize = 36;
    let point = |i: usize| {
        let angle = i as f32 / SLICES as f32 * std::f32::consts::TAU;
        centre + radius * vec2(angle.cos(), angle.sin())
    };
    for i in 0..SLICES {
        let hue = (i as f32 + 0.5) / SLICES as f32;
        let colour = Color32::from(egui::ecolor::Hsva::new(hue, 0.75, 0.95, 1.0));
        painter.add(egui::Shape::convex_polygon(
            vec![centre, point(i), point(i + 1)],
            colour,
            Stroke::new(0.6, colour),
        ));
    }
}

/// The hint of the on-top row: the key that switches it, if any.
fn on_top_hint() -> String {
    let key = crate::keymap::shortcut(crate::input::Cmd::CycleOnTop);
    if key.is_empty() {
        String::new()
    } else {
        tr!(format!("{key} switches it."), format!("{key} переключает."))
    }
}

/// The hint of the Resume row, with the key that goes to the start.
fn resume_hint() -> String {
    let text = tr!(
        "A file of 5 minutes or more opened again starts where it was left.",
        "Файл длиной от 5 минут при повторном открытии начинается с того места, где остановился."
    );
    let key = crate::keymap::shortcut(crate::input::Cmd::Start);
    if key.is_empty() {
        text.to_owned()
    } else {
        tr!(
            format!("{text} {key} goes to the beginning."),
            format!("{text} {key} переходит к началу.")
        )
    }
}

/// The hint of the layout row: what each one shows and the keys.
fn interface_hint() -> String {
    let mut hint = tr!(
        "Without the title row, the title row comes over the picture's top while the pointer is there; compact puts the control bar in one row; with only the picture, the control bar comes over its bottom too.",
        "Без строки заголовка она появляется поверх картинки, когда указатель у её верхнего края; компактный вид укладывает панель управления в одну строку; в режиме «только видео» поверх нижнего края появляется и панель управления."
    )
    .to_owned();
    let cycle = crate::keymap::shortcut(crate::input::Cmd::CycleInterface);
    if !cycle.is_empty() {
        hint += &tr!(
            format!(" {cycle} switches them."),
            format!(" {cycle} переключает.")
        );
    }
    let bare = crate::keymap::shortcut(crate::input::Cmd::ToggleInterface);
    if !bare.is_empty() {
        hint += &tr!(
            format!(" {bare}: only the picture and back."),
            format!(" {bare} — только видео и обратно.")
        );
    }
    hint
}

/// An on/off switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
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
                style::accent_hover()
            } else {
                style::accent()
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

fn general(ui: &mut Ui, settings: &mut Settings, changed: &mut Changed) {
    section(ui, tr!("Interface", "Интерфейс"), |ui| {
        row(
            ui,
            tr!("Language", "Язык"),
            Some(tr!(
                "\"As in Windows\" follows the language of Windows.",
                "«Как в Windows» следует языку Windows."
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
            tr!("Layout", "Вид интерфейса"),
            Some(&interface_hint()),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_interface")
                    .selected_text(settings.interface.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for i in Interface::ALL {
                            ui.selectable_value(&mut settings.interface, i, i.label());
                        }
                    });
            },
        );
        divider(ui);
        row(
            ui,
            tr!("Accent colour", "Акцентный цвет"),
            Some(tr!(
                "The Play button, the seek bar, the switches, the playing file and the icon of the window.",
                "Кнопка воспроизведения, полоса перемотки, переключатели, текущий файл и значок окна."
            )),
            ACCENTS_WIDTH,
            |ui| changed.accent |= accents(ui, &mut settings.accent),
        );
        divider(ui);
        row(
            ui,
            tr!("Snap to the screen's centre", "Притягивать к центру экрана"),
            Some(tr!(
                "While the window is moved, its centre is drawn to the screen's centre, across or up and down, when it comes within a few pixels; moved on, it comes away.",
                "При перетаскивании центр окна примагничивается к центру экрана по горизонтали или по вертикали, когда подходит к нему на несколько пикселей; если тянуть дальше, окно отрывается."
            )),
            50.0,
            |ui| switch(ui, &mut settings.snap_to_centre),
        );
        divider(ui);
        row(
            ui,
            tr!("Over the other windows", "Поверх других окон"),
            Some(&on_top_hint())
                .filter(|h| !h.is_empty())
                .map(String::as_str),
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
        divider(ui);
        row(
            ui,
            tr!("Window when a file opens", "Окно при открытии файла"),
            Some(tr!(
                "Fitted to the picture: the black bars taken off, the width kept. 100 %: a pixel of the video on a pixel of the screen, made smaller if the screen is. Not while maximized or in fullscreen.",
                "По изображению — без чёрных полос, ширина сохраняется. 100 % — пиксель видео на пиксель экрана, меньше, если экран не вмещает. Не действует в развёрнутом окне и полноэкранном режиме."
            )),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_window_on_open")
                    .selected_text(settings.window_on_open.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for w in WindowOnOpen::ALL {
                            ui.selectable_value(&mut settings.window_on_open, w, w.label());
                        }
                    });
            },
        );
    });
    section(ui, tr!("Playlist", "Плейлист"), |ui| {
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
        divider(ui);
        row(
            ui,
            tr!("Channel logos", "Логотипы каналов"),
            Some(tr!(
                "The pictures an IPTV list (M3U) gives its channels are fetched from their sites, only over HTTPS and never from the local network, and kept for 30 days.",
                "Картинки, которые плейлист IPTV (M3U) указывает для каналов, загружаются с их сайтов — только по HTTPS и не из локальной сети — и хранятся 30 дней."
            )),
            50.0,
            |ui| switch(ui, &mut settings.channel_logos),
        );
    });
    osd_section(ui, settings, changed);
}

fn playback(ui: &mut Ui, settings: &mut Settings, changed: &mut Changed) {
    section(ui, tr!("History", "История"), |ui| {
        row(
            ui,
            tr!(
                "Go on from where it stopped",
                "Продолжать с места остановки"
            ),
            Some(&resume_hint()),
            50.0,
            |ui| switch(ui, &mut settings.resume_playback),
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
            |ui| ui.button(tr!("Forget all", "Забыть всё")).clicked(),
        );
    });
    section(ui, tr!("Seek steps", "Шаги перемотки"), |ui| {
        row(
            ui,
            &key_pair(
                Cmd::SeekShort(-1.0),
                Cmd::SeekShort(1.0),
                tr!("Short step", "Короткий шаг"),
            ),
            Some(tr!(
                "On keyframes: instant, playback goes on from the keyframe nearest to the target.",
                "По ключевым кадрам: мгновенно, воспроизведение продолжается с ключевого кадра, ближайшего к цели."
            )),
            120.0,
            |ui| step(ui, &mut settings.seek_short),
        );
        divider(ui);
        row(
            ui,
            &key_pair(
                Cmd::SeekLong(-1.0),
                Cmd::SeekLong(1.0),
                tr!("Long step", "Длинный шаг"),
            ),
            Some(tr!(
                "On keyframes, as the short one.",
                "По ключевым кадрам, как и короткий."
            )),
            120.0,
            |ui| step(ui, &mut settings.seek_long),
        );
    });
    section(ui, tr!("Chapters", "Главы"), |ui| {
        row(
            ui,
            tr!("Marks on the seek bar", "Метки на полосе перемотки"),
            Some(tr!(
                "Cuts in the bar where the chapters start; also in the context menu. The chapters themselves are listed under the playlist.",
                "Разрывы на полосе в начале глав; переключаются и в контекстном меню. Сами главы перечислены под плейлистом."
            )),
            50.0,
            |ui| switch(ui, &mut settings.chapter_marks),
        );
    });
    section(
        ui,
        tr!("Incomplete files", "Неполные файлы"),
        |ui| {
            row(
                ui,
                tr!("Stay in the file", "Оставаться в файле"),
                Some(tr!(
                    "Where the part of the file that is there ends (a download not finished), playback stops, paused, with a message on the OSD, and Play goes on from where there was data; off, the file ends and the next one in the playlist plays.",
                    "Там, где кончается имеющаяся часть файла (загрузка не закончена), воспроизведение останавливается на паузе с сообщением на OSD, а Play продолжает с места, где данные были; выключено — файл заканчивается и воспроизводится следующий в плейлисте."
                )),
                50.0,
                |ui| switch(ui, &mut settings.stay_in_incomplete),
            );
        },
    );
    section(ui, tr!("Media keys", "Медиаклавиши"), |ui| {
        row(
            ui,
            tr!(
                "Play, pause, stop, previous, next",
                "Воспроизведение, пауза, стоп, предыдущий, следующий"
            ),
            Some(tr!(
                "The media keys of a keyboard, a headset or a remote. Always: also while another window is active, through Windows' media controls, whose flyout over the volume shows the title. Off: the player takes none, they go on to Windows and the other programs.",
                "Медиаклавиши клавиатуры, гарнитуры или пульта. «Всегда» — также когда активно другое окно, через системные элементы управления мультимедиа, всплывающее окно которых над регулятором громкости показывает название. «Выключены» — плеер не перехватывает их, они достаются Windows и другим программам."
            )),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_media_keys")
                    .selected_text(settings.media_keys.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for m in MediaKeys::ALL {
                            ui.selectable_value(&mut settings.media_keys, m, m.label());
                        }
                    });
            },
        );
    });
}

/// The languages of the sound and the subtitles, which subtitles show,
/// the subtitle files loaded, and the tracks remembered or chosen alike.
fn tracks_page(ui: &mut Ui, settings: &mut Settings) {
    use crate::tracks::{SubAuto, SubMode};
    let prefs = &mut settings.track_prefs;
    section(ui, tr!("Sound", "Звук"), |ui| {
        row(
            ui,
            tr!("Languages", "Языки"),
            Some(tr!(
                "A file opened for the first time plays the first of these languages it has; none of them, or none chosen: the track the file marks.",
                "Файл, открытый впервые, играет первый из этих языков, который в нём есть; если ни одного нет или они не выбраны — дорожку, отмеченную в файле."
            )),
            LANGUAGES_WIDTH,
            |ui| language_list(ui, "audio_languages", &mut prefs.audio),
        );
    });
    section(ui, tr!("Subtitles", "Субтитры"), |ui| {
        row(
            ui,
            tr!("Shown", "Показывать"),
            Some(tr!(
                "Which subtitles a file opened for the first time shows. Forced ones carry the signs and the lines in other languages.",
                "Какие субтитры показывает файл, открытый впервые. Принудительные содержат надписи и реплики на других языках."
            )),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_sub_mode")
                    .selected_text(prefs.sub_mode.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for m in SubMode::ALL {
                            ui.selectable_value(&mut prefs.sub_mode, m, m.label());
                        }
                    });
            },
        );
        divider(ui);
        let by_language = prefs.sub_mode == SubMode::Languages;
        row(
            ui,
            tr!("Languages", "Языки"),
            Some(tr!(
                "With \"In the languages below\": the first of them the file has; none of them, only forced subtitles in the sound's language.",
                "При «На языках ниже»: первый из них, который есть в файле; если ни одного нет — только принудительные субтитры на языке звука."
            )),
            LANGUAGES_WIDTH,
            |ui| {
                ui.add_enabled_ui(by_language, |ui| {
                    language_list(ui, "sub_languages", &mut prefs.subs);
                });
            },
        );
        divider(ui);
        row(
            ui,
            tr!("Load from the folder", "Подхватывать из папки"),
            Some(tr!(
                "Subtitle files beside the video, and in its Subs, Subtitles or Sub folder, loaded with it.",
                "Файлы субтитров рядом с видео и в его папке Subs, Subtitles или Sub, загружаемые вместе с ним."
            )),
            230.0,
            |ui| {
                egui::ComboBox::from_id_salt("settings_sub_auto")
                    .selected_text(prefs.sub_auto.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for a in SubAuto::ALL {
                            ui.selectable_value(&mut prefs.sub_auto, a, a.label());
                        }
                    });
            },
        );
    });
    section(ui, tr!("Choice", "Выбор дорожек"), |ui| {
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
                "A file opened for the first time plays the audio track and the subtitles most like those last chosen by hand, by title and language: the same voice-over in the next episode of a series. Before the languages above.",
                "Файл, открытый впервые, воспроизводится со звуковой дорожкой и субтитрами, больше всего похожими на последние выбранные вручную, по названию и языку: та же озвучка в следующей серии сериала. Важнее языков выше."
            )),
            50.0,
            |ui| switch(ui, &mut settings.prefer_last_tracks),
        );
    });
}

/// The width of three language lists in a row.
const LANGUAGES_WIDTH: f32 = 3.0 * LANGUAGE_WIDTH + 2.0 * 6.0;
const LANGUAGE_WIDTH: f32 = 104.0;

/// Up to `TrackPrefs::MAX` languages in order, a list each; "—" takes a
/// language out, and the ones after it move up.
fn language_list(ui: &mut Ui, id: &str, list: &mut Vec<String>) {
    use crate::tracks::{TrackPrefs, languages};
    let all = languages(crate::i18n::lang());
    let name = |code: &str| {
        all.iter()
            .find(|(c, _)| *c == code)
            .map_or(code.to_owned(), |(_, n)| (*n).to_owned())
    };
    ui.spacing_mut().item_spacing.x = 6.0;
    ui.horizontal(|ui| {
        for i in 0..TrackPrefs::MAX {
            // A list after the first empty one waits for it.
            let shown = i <= list.len();
            let mut chosen = list.get(i).cloned().unwrap_or_default();
            ui.add_enabled_ui(shown, |ui| {
                egui::ComboBox::from_id_salt((id, i))
                    .selected_text(if chosen.is_empty() {
                        "—".to_owned()
                    } else {
                        name(&chosen)
                    })
                    .width(LANGUAGE_WIDTH)
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, String::new(), "—");
                        for (code, label) in &all {
                            // Each language once.
                            let taken = list.iter().enumerate().any(|(j, c)| j != i && c == code);
                            if !taken {
                                ui.selectable_value(&mut chosen, (*code).to_owned(), *label);
                            }
                        }
                    });
            });
            if chosen != list.get(i).cloned().unwrap_or_default() {
                if chosen.is_empty() {
                    list.truncate(i);
                } else if i < list.len() {
                    list[i] = chosen;
                } else {
                    list.push(chosen);
                }
            }
        }
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

/// Whether the chosen output device (empty: Windows' default) is there.
pub fn device_present(chosen: &str, devices: &[Device]) -> bool {
    chosen.is_empty() || devices.iter().any(|d| d.name == chosen)
}

/// The list of output devices, Windows' default first, `width` wide;
/// `chosen` is mpv's name of the device (empty: Windows' default). The app
/// gives mpv the device (`App::follow_audio_device`). Also on the Sound tab
/// of the Video, sound and subtitles panel.
pub fn device_choice(ui: &mut Ui, id: &str, chosen: &mut String, devices: &[Device], width: f32) {
    let system = tr!("Windows' default", "Устройство Windows по умолчанию");
    let name = if chosen.is_empty() {
        system.to_owned()
    } else {
        devices
            .iter()
            .find(|d| &d.name == chosen)
            .map_or_else(|| chosen.clone(), |d| d.description.clone())
    };
    // About 8 points a character.
    let chars = ((width - 20.0) / 8.0) as usize;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shorten(&name, chars))
        .width(width)
        .show_ui(ui, |ui| {
            ui.selectable_value(chosen, String::new(), system);
            for d in devices {
                ui.selectable_value(chosen, d.name.clone(), &d.description);
            }
        });
}

/// The output device and channels, the volume's boost and normalization.
/// The keys of a pair of commands as a row's name (`← / →`), the keys
/// changed on the Keys page included; `fallback` when neither has one.
fn key_pair(back: Cmd, forward: Cmd, fallback: &str) -> String {
    let keys: Vec<String> = [back, forward]
        .into_iter()
        .map(shortcut)
        .filter(|k| !k.is_empty())
        .collect();
    if keys.is_empty() {
        fallback.to_owned()
    } else {
        keys.join(" / ")
    }
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

/// The font and the size of the OSD line, with a sample.
fn osd_section(ui: &mut Ui, settings: &mut Settings, changed: &mut Changed) {
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
                |ui| font_choice(ui, "osd_font", &settings.osd_font, 220.0),
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
            divider(ui);
            osd_preview(ui, settings.osd_scale);
        },
    );
}

/// A line of the OSD in its font and size on a dark ground, as over a
/// picture of medium height (the font chosen shows from the next frame).
fn osd_preview(ui: &mut Ui, scale: f64) {
    // The OSD's size over a picture about 570 points high.
    let size = 22.0 * (scale / 100.0) as f32;
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), (size * 2.2).max(40.0)),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, Color32::from_rgb(0x14, 0x16, 0x1a));
    let text = tr!("[ Volume 80% ]", "[ Громкость 80% ]");
    let pos = rect.left_top() + vec2(size * 0.6, (rect.height() - size * 1.2) / 2.0);
    super::osd::paint_text(&painter, pos, text, size, 1.0);
    ui.add_space(8.0);
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
/// frame: `Some(None)` is the default. Also the panel's Subtitles tab.
pub(super) fn font_choice(
    ui: &mut Ui,
    id: &str,
    current: &str,
    width: f32,
) -> Option<Option<&'static Family>> {
    let default = tr!("Default", "По умолчанию");
    let shown = if current.is_empty() { default } else { current };
    let filter_id = ui.id().with((id, "filter"));
    let mut picked = None;
    let list = egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(width)
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
    // Closed, with a pick or without: the next opening starts afresh.
    if picked.is_some() || list.inner.is_none() {
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
            if let Some(date) = check_date(updates.last_check) {
                ui.weak(tr!(
                    format!("Last checked: {date}"),
                    format!("Последняя проверка: {date}")
                ));
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

/// When updates were last checked for, as the user reads dates; none
/// before the first check.
fn check_date(unix_seconds: u64) -> Option<String> {
    if unix_seconds == 0 {
        return None;
    }
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_seconds);
    let local = crate::win::local_time(time)?;
    Some(crate::format::date_in(crate::i18n::lang(), local))
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

impl SettingsDialog {
    /// The next press while a key is being chosen: a key with its
    /// modifiers, or a side button of the mouse; Esc or another click
    /// gives up. The keys pressed meanwhile reach no widget and do not
    /// close the window.
    fn capture_key(&mut self, ctx: &egui::Context, keys: &mut Keymap, changed: &mut Changed) {
        let Some((action, slot)) = self.capture else {
            return;
        };
        let (events, modifiers) = ctx.input(|i| (i.events.clone(), i.modifiers));
        for event in &events {
            match event {
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } => {
                    self.capture = None;
                    break;
                }
                egui::Event::PointerButton {
                    button,
                    pressed: true,
                    ..
                } if Chord::of_button(*button, egui::Modifiers::NONE).is_none() => {
                    self.gave_up = self.capture.take();
                    break;
                }
                _ => {}
            }
            if let Some((chord, false)) = Chord::of_event(event, modifiers) {
                let before = keys.clone();
                self.taken = keys
                    .assign(action, slot, chord)
                    .map(|other| (chord, other, before));
                self.capture = None;
                changed.keys = true;
                break;
            }
        }
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                )
            });
        });
    }

    /// The keys page: every action with its keys; a click on a key waits
    /// for the one to take its place, `+` for one more, the cross removes
    /// it.
    fn keys_page(&mut self, ui: &mut Ui, keys: &mut Keymap, changed: &mut Changed) {
        section(ui, tr!("Choosing keys", "Выбор клавиш"), |ui| {
            row(
                ui,
                tr!("Default keys", "Клавиши по умолчанию"),
                Some(tr!(
                    "A click on a key waits for another to take its place, + for one more; Esc gives up. A key another action had is taken from it.",
                    "Щелчок по клавише ждёт нажатия новой на её место, «+» — ещё одной; Esc — отмена. Клавиша другого действия переходит к этому."
                )),
                150.0,
                |ui| {
                    if ui
                        .add_enabled(
                            !keys.is_default(),
                            egui::Button::new(tr!("Reset the keys", "Сбросить клавиши")),
                        )
                        .clicked()
                    {
                        *keys = Keymap::default();
                        self.capture = None;
                        self.taken = None;
                        changed.keys = true;
                    }
                },
            );
        });
        let mut undo = None;
        if let Some((chord, other, before)) = &self.taken {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr!(
                        format!("{} no longer does “{}”.", chord.label(), other.name()),
                        format!("{} больше не делает «{}».", chord.label(), other.name())
                    ))
                    .size(12.5)
                    .color(style::OSD),
                );
                if ui.small_button(tr!("Undo", "Отменить")).clicked() {
                    undo = Some(before.clone());
                }
            });
        }
        if let Some(before) = undo {
            *keys = before;
            self.taken = None;
            changed.keys = true;
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.key_filter)
                    .hint_text(tr!("Find an action or a key", "Найти действие или клавишу"))
                    .desired_width(260.0),
            );
            if !self.key_filter.is_empty() && ui.small_button("×").clicked() {
                self.key_filter.clear();
            }
        });
        let filter = self.key_filter.trim().to_lowercase();
        let shown: Vec<Action> = Action::ALL
            .iter()
            .copied()
            .filter(|&action| {
                filter.is_empty()
                    || action.name().to_lowercase().contains(&filter)
                    || keys
                        .chords(action)
                        .iter()
                        .any(|c| c.label().to_lowercase().contains(&filter))
            })
            .collect();
        let mut any = false;
        for group in Group::ALL {
            let actions: Vec<Action> = shown
                .iter()
                .copied()
                .filter(|a| a.group() == group)
                .collect();
            if actions.is_empty() {
                continue;
            }
            any = true;
            section(ui, group.title(), |ui| {
                for (i, &action) in actions.iter().enumerate() {
                    if i > 0 {
                        divider(ui);
                    }
                    // Many or long keys take more room from the name.
                    let width = self
                        .chips_width(ui, keys, action)
                        .clamp(KEYS_WIDTH, (ui.available_width() - 120.0).max(KEYS_WIDTH));
                    row(ui, action.name(), None, width, |ui| {
                        self.key_chips(ui, keys, action, changed);
                    });
                }
            });
        }
        if !any {
            note(ui, tr!("Nothing found.", "Ничего не найдено."));
        }
        note(
            ui,
            tr!(
                "The mouse wheel changes the volume, a double click switches fullscreen, a middle click shows or hides the playlist; the media keys play, pause, stop and change files. Ctrl+V and Shift+Insert cannot be chosen, and Ctrl+Insert is taken as Ctrl+C.",
                "Колесо мыши меняет громкость, двойной щелчок переключает полноэкранный режим, средний щелчок показывает или скрывает плейлист; мультимедийные клавиши запускают, приостанавливают, останавливают и переключают файлы. Ctrl+V и Shift+Insert выбрать нельзя, а Ctrl+Insert считается сочетанием Ctrl+C."
            ),
        );
    }

    /// The width the keys of `action` take, as [`key_chip`] draws them.
    fn chips_width(&self, ui: &Ui, keys: &Keymap, action: Action) -> f32 {
        const GAP: f32 = 6.0;
        let chip = |label: String, waiting: bool| {
            let text =
                ui.painter()
                    .layout_no_wrap(label, FontId::proportional(13.0), style::TEXT_BRIGHT);
            text.size().x + 18.0 + if waiting { 0.0 } else { 18.0 } + GAP
        };
        let mut width = 26.0 + GAP;
        if self.capture == Some((action, None)) {
            width += chip(waiting(), true);
        }
        for (slot, chord) in keys.chords(action).iter().enumerate() {
            width += if self.capture == Some((action, Some(slot))) {
                chip(waiting(), true)
            } else {
                chip(chord.label(), false)
            };
        }
        width
    }

    /// The keys of `action` in a right-to-left layout: `+` first, then the
    /// keys from the last.
    fn key_chips(&mut self, ui: &mut Ui, keys: &mut Keymap, action: Action, changed: &mut Changed) {
        ui.spacing_mut().item_spacing.x = 6.0;
        let add = ui
            .add(egui::Button::new("+").min_size(vec2(26.0, KEY_HEIGHT)))
            .on_hover_text(tr!("Add a key", "Добавить клавишу"));
        if add.clicked() && self.gave_up != Some((action, None)) {
            self.capture = Some((action, None));
            self.taken = None;
        }
        if self.capture == Some((action, None)) {
            key_chip(ui, (action, usize::MAX), &waiting(), true);
        }
        let chords = keys.chords(action).to_vec();
        for (slot, chord) in chords.iter().enumerate().rev() {
            let capturing = self.capture == Some((action, Some(slot)));
            let label = if capturing { waiting() } else { chord.label() };
            match key_chip(ui, (action, slot), &label, capturing) {
                Some(Chip::Change) if self.gave_up != Some((action, Some(slot))) => {
                    self.capture = Some((action, Some(slot)));
                    self.taken = None;
                }
                Some(Chip::Remove) => {
                    keys.remove(action, slot);
                    self.capture = None;
                    self.taken = None;
                    changed.keys = true;
                }
                _ => {}
            }
        }
    }
}

/// The width of the keys of a row.
const KEYS_WIDTH: f32 = 300.0;
/// The height of a key.
const KEY_HEIGHT: f32 = 24.0;
/// A key's ground under the pointer.
const KEY_HOVER: Color32 = Color32::from_rgb(0x48, 0x4d, 0x58);

/// What a key on the keys page was clicked for.
enum Chip {
    Change,
    Remove,
}

/// The label of a key waiting for a press.
fn waiting() -> String {
    tr!("Press a key…", "Нажмите клавишу…").to_owned()
}

/// A key on a keycap ground with a cross that removes it; while it waits
/// for a press, in the accent colour without the cross.
fn key_chip(ui: &mut Ui, id: (Action, usize), label: &str, waiting: bool) -> Option<Chip> {
    const CROSS: f32 = 18.0;
    let text = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(13.0),
        style::TEXT_BRIGHT,
    );
    let cross_width = if waiting { 0.0 } else { CROSS };
    let size = vec2(text.size().x + 18.0 + cross_width, KEY_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let key_rect = Rect::from_min_max(rect.min, pos2(rect.right() - cross_width, rect.bottom()));
    let id = ui.id().with(("key_chip", id));
    let key = ui
        .interact(key_rect, id, Sense::CLICK)
        .on_hover_text(if waiting {
            tr!("Esc or a click gives up", "Esc или щелчок — отмена")
        } else {
            tr!("Choose another key", "Выбрать другую клавишу")
        });
    let cross = (!waiting).then(|| {
        let r = Rect::from_min_max(pos2(key_rect.right(), rect.top()), rect.max);
        ui.interact(r, id.with("remove"), Sense::CLICK)
            .on_hover_text(tr!("Remove the key", "Убрать клавишу"))
    });
    let hovered = key.hovered() || cross.as_ref().is_some_and(Response::hovered);
    let fill = if waiting {
        style::accent()
    } else if hovered {
        KEY_HOVER
    } else {
        style::KEY_BG
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 5.0, fill);
    painter.galley(
        pos2(rect.left() + 9.0, rect.center().y - text.size().y / 2.0),
        text,
        style::TEXT_BRIGHT,
    );
    if let Some(cross) = &cross {
        let c = if cross.hovered() {
            style::TEXT_BRIGHT
        } else {
            style::TEXT_WEAK
        };
        let o = pos2(cross.rect.center().x - 2.0, cross.rect.center().y);
        let d = 3.5;
        let stroke = Stroke::new(1.4, c);
        painter.line_segment([o + vec2(-d, -d), o + vec2(d, d)], stroke);
        painter.line_segment([o + vec2(-d, d), o + vec2(d, -d)], stroke);
    }
    if cross.is_some_and(|c| c.clicked()) {
        Some(Chip::Remove)
    } else if key.clicked() {
        Some(Chip::Change)
    } else {
        None
    }
}

/// A seek step in seconds.
fn step(ui: &mut Ui, value: &mut f64) {
    ui.spacing_mut().interact_size.x = 80.0;
    ui.add(
        egui::DragValue::new(value)
            .range(MIN_STEP..=MAX_STEP)
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
    changed.osd_font |= settings.osd_font != d.osd_font;
    if settings.language != d.language {
        crate::i18n::set_lang(d.language.resolve());
    }
    settings.language = d.language;
    settings.seek_short = d.seek_short;
    settings.seek_long = d.seek_long;
    settings.hwdec = d.hwdec;
    settings.fill_playlist = d.fill_playlist;
    settings.channel_logos = d.channel_logos;
    settings.chapter_marks = d.chapter_marks;
    settings.resume_playback = d.resume_playback;
    settings.remember_tracks = d.remember_tracks;
    settings.track_prefs = d.track_prefs;
    settings.prefer_last_tracks = d.prefer_last_tracks;
    settings.stay_in_incomplete = d.stay_in_incomplete;
    settings.remember_recent = d.remember_recent;
    settings.fullscreen_transparency = d.fullscreen_transparency;
    settings.fullscreen_title = d.fullscreen_title;
    settings.snap_to_centre = d.snap_to_centre;
    settings.on_top = d.on_top;
    settings.window_on_open = d.window_on_open;
    settings.media_keys = d.media_keys;
    settings.interface = d.interface;
    changed.accent |= settings.accent != d.accent;
    settings.accent = d.accent;
    changed.keys |= settings.keys != d.keys;
    settings.keys = d.keys;
    settings.osd_font = d.osd_font;
    settings.osd_font_file = d.osd_font_file;
    settings.osd_scale = d.osd_scale;
    updates.enabled = d.check_updates;
}
