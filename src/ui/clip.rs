//! The Save the A-B stretch window (`Ctrl+Shift+S`, the A-B repeat
//! submenu): the file it goes to, the way (as it is, quick and on
//! keyframes; or encoded, exact, with the codec, the card or the
//! processor, the quality and the subtitles burnt in), then the progress
//! of the saving (`clip.rs`) with Cancel. Playback goes on meanwhile.

use std::path::{Path, PathBuf};

use egui::{RichText, vec2};

use super::style;
use crate::clip::{Choice, Codec, Failure, Job, QUALITY_RANGE, Request, Subs};

/// What the window needs of the file playing to make a request.
pub struct Stretch {
    pub input: PathBuf,
    pub from: f64,
    pub to: f64,
    pub audio: Option<i64>,
    pub subs: Subs,
    pub sub_style: Vec<(&'static str, String)>,
    /// The folder the file goes to at first.
    pub folder: PathBuf,
}

/// What the app does for the window.
pub enum Action {
    /// Show a Save dialog for the file, starting at this one.
    Browse(PathBuf),
    /// Saved there.
    Saved(PathBuf),
}

#[derive(Default)]
pub struct ClipDialog {
    stretch: Option<Stretch>,
    /// The file it goes to, as typed.
    path: String,
    job: Option<Job>,
    error: Option<String>,
}

impl ClipDialog {
    pub fn open(&mut self, stretch: Stretch, choice: Choice) {
        if self.job.is_some() {
            return;
        }
        let name = crate::clip::file_name(
            &stretch.folder,
            &stretch.input,
            stretch.from,
            stretch.to,
            choice.mode().extension(),
        );
        self.path = name.to_string_lossy().into_owned();
        self.stretch = Some(stretch);
        self.error = None;
    }

    pub fn is_open(&self) -> bool {
        self.stretch.is_some()
    }

    /// The file chosen in the Save dialog.
    pub fn set_path(&mut self, path: &Path) {
        self.path = path.to_string_lossy().into_owned();
    }

    /// Stop a saving under way (the program closes).
    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, choice: &mut Choice) -> Option<Action> {
        let stretch = self.stretch.as_ref()?;
        let mut action = None;
        if let Some(result) = self.job.as_ref().and_then(Job::take) {
            let job = self.job.take().expect("taken from it");
            match result {
                Ok(()) => {
                    self.stretch = None;
                    return Some(Action::Saved(job.output));
                }
                Err(Failure::Cancelled) => {}
                Err(Failure::TooLong) => {
                    self.error = Some(
                        tr!(
                            "The stretch is too long to save as it is: encode it, or make it shorter.",
                            "Фрагмент слишком длинный, чтобы сохранить его как есть: перекодируйте его или сократите."
                        )
                        .to_owned(),
                    );
                }
                Err(Failure::Mpv(e)) => {
                    log::warn!("cannot save the stretch: {e}");
                    self.error = Some(tr!(
                        format!("Cannot save it: {e}"),
                        format!("Не удалось сохранить: {e}")
                    ));
                }
            }
        }
        let mut close = false;
        let mut start = false;
        let lang = crate::i18n::lang();
        let modal = egui::Modal::new(egui::Id::new("save_clip")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(
                RichText::new(tr!("Save the A-B stretch", "Сохранить фрагмент A-B"))
                    .size(17.0)
                    .color(style::TEXT_BRIGHT),
            );
            let span = stretch.to - stretch.from;
            ui.label(
                RichText::new(format!(
                    "{} – {}  ({})",
                    crate::format::clock_tenths_in(lang, stretch.from),
                    crate::format::clock_tenths_in(lang, stretch.to),
                    crate::format::time(Some(span), span >= 3600.0)
                ))
                .color(style::TEXT_WEAK),
            );
            ui.add_space(8.0);
            if let Some(job) = &self.job {
                ui.label(tr!("Saving…", "Сохраняется…"));
                ui.add_space(4.0);
                ui.add(egui::ProgressBar::new(job.progress()).show_percentage());
                ui.add_space(10.0);
                if ui.button(tr!("Cancel", "Отмена")).clicked() {
                    job.cancel();
                }
                return;
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.path)
                        .desired_width(ui.available_width() - 36.0),
                );
                let browse = ui
                    .button("…")
                    .on_hover_text(tr!("Choose the file", "Выбрать файл"));
                if browse.clicked() {
                    action = Some(Action::Browse(PathBuf::from(&self.path)));
                }
            });
            ui.add_space(6.0);
            let before = choice.mode().extension();
            ui.radio_value(
                &mut choice.copy,
                true,
                tr!("As it is", "Как есть"),
            );
            ui.label(
                RichText::new(tr!(
                    "Quick and in the quality it was; it starts and ends on keyframes, a little before and after the points, into an MKV.",
                    "Быстро и в исходном качестве; начало и конец — по ключевым кадрам, чуть раньше и позже точек, в MKV."
                ))
                .size(12.0)
                .color(style::TEXT_WEAK),
            );
            ui.radio_value(
                &mut choice.copy,
                false,
                tr!("Encoded", "Перекодировать"),
            );
            ui.label(
                RichText::new(tr!(
                    "Exact to the frame, into an MP4; takes longer.",
                    "Точно по кадрам, в MP4; дольше."
                ))
                .size(12.0)
                .color(style::TEXT_WEAK),
            );
            if !choice.copy {
                ui.add_space(4.0);
                let e = &mut choice.encode;
                egui::Grid::new("clip_encode")
                    .num_columns(2)
                    .spacing(vec2(12.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(tr!("Codec", "Кодек"));
                        egui::ComboBox::from_id_salt("clip_codec")
                            .selected_text(e.codec.label())
                            .show_ui(ui, |ui| {
                                for codec in Codec::ALL {
                                    ui.selectable_value(&mut e.codec, codec, codec.label());
                                }
                            });
                        ui.end_row();
                        ui.label(tr!("Encoder", "Кодировщик"));
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut e.nvenc, false, tr!("Processor", "Процессор"));
                            ui.radio_value(
                                &mut e.nvenc,
                                true,
                                tr!("NVIDIA card (NVENC)", "Видеокарта NVIDIA (NVENC)"),
                            );
                        });
                        ui.end_row();
                        ui.label(tr!("Quality", "Качество"))
                            .on_hover_text(tr!(
                                "x264's and x265's CRF, NVENC's CQ: lower is better and larger",
                                "CRF у x264 и x265, CQ у NVENC: меньше — лучше и крупнее файл"
                            ));
                        ui.add(egui::Slider::new(&mut e.quality, QUALITY_RANGE).text(tr!(
                            "lower is better",
                            "меньше — лучше"
                        )));
                        ui.end_row();
                    });
                ui.add_space(2.0);
                let subs = stretch.subs != Subs::None;
                ui.add_enabled(
                    subs,
                    egui::Checkbox::new(
                        &mut e.burn_subs,
                        tr!(
                            "Burn the subtitles shown into the picture",
                            "Впечатать показанные субтитры в изображение"
                        ),
                    ),
                )
                .on_disabled_hover_text(tr!("No subtitles are shown", "Субтитры не показаны"));
            } else if matches!(stretch.subs, Subs::File { .. }) {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(tr!(
                        "Subtitles from a file of their own are not taken along as it is; encoded, they can be burnt in.",
                        "Субтитры из отдельного файла при сохранении как есть не попадут; при перекодировании их можно впечатать."
                    ))
                    .size(12.0)
                    .color(style::TEXT_WEAK),
                );
            }
            // The extension follows the way.
            let after = choice.mode().extension();
            if after != before
                && let Some(stem) = self.path.strip_suffix(&format!(".{before}"))
            {
                self.path = format!("{stem}.{after}");
            }
            if let Some(error) = &self.error {
                ui.add_space(6.0);
                ui.label(RichText::new(error).color(style::OSD));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let save = egui::Button::new(tr!("Save", "Сохранить")).min_size(vec2(90.0, 0.0));
                if ui.add_enabled(!self.path.trim().is_empty(), save).clicked() {
                    start = true;
                }
                if ui.button(tr!("Close", "Закрыть")).clicked() {
                    close = true;
                }
            });
        });
        if start {
            let request = Request {
                input: stretch.input.clone(),
                from: stretch.from,
                to: stretch.to,
                output: PathBuf::from(self.path.trim()),
                mode: choice.mode(),
                audio: stretch.audio,
                subs: stretch.subs.clone(),
                sub_style: stretch.sub_style.clone(),
            };
            let wake = ctx.clone();
            self.error = None;
            match Job::start(request, move || wake.request_repaint()) {
                Ok(job) => self.job = Some(job),
                Err(e) => self.error = Some(e),
            }
        }
        // Closed only while nothing is being saved.
        if self.job.is_none() && (close || modal.should_close()) {
            self.stretch = None;
        }
        action
    }
}
