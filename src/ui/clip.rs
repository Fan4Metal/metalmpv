//! The Save the A-B stretch window (`Ctrl+Shift+S`, the A-B repeat
//! submenu): the file it goes to, how it is encoded (the codec, the card or
//! the processor, the quality, the subtitles burnt in), then the progress
//! of the saving (`clip.rs`) with Cancel. Playback goes on meanwhile; while
//! it saves, the window can be hidden and the end comes on the OSD.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{RichText, vec2};

use super::style;
use crate::clip::{Codec, Encode, Failure, Job, QUALITY_RANGE, Request, Subs};

/// What the window needs of the file playing to make a request.
pub struct Stretch {
    pub input: PathBuf,
    pub from: f64,
    pub to: f64,
    pub audio: Option<i64>,
    /// The audio track's channels.
    pub channels: Option<i64>,
    pub subs: Subs,
    pub sub_style: Vec<(&'static str, String)>,
    pub audio_delay: f64,
    pub sub_delay: f64,
    /// mpv's `deinterlace` of the player.
    pub deinterlace: &'static str,
    /// The folder the file goes to at first, and a relative path's.
    pub folder: PathBuf,
}

/// What the app does for the window.
pub enum Action {
    /// Show a Save dialog for the file, starting at this one.
    Browse(PathBuf),
    /// Saved there.
    Saved(PathBuf),
    /// Not saved, why (the window hidden meanwhile).
    Failed(String),
}

#[derive(Default)]
pub struct ClipDialog {
    stretch: Option<Stretch>,
    /// The file it goes to, as typed.
    path: String,
    /// The file the Save dialog was told to replace.
    replace: Option<PathBuf>,
    job: Option<Job>,
    /// Hidden while it saves: the player is free meanwhile.
    hidden: bool,
    error: Option<String>,
}

impl ClipDialog {
    /// Show the window for `stretch`; while one is being saved, show that
    /// one again.
    pub fn open(&mut self, stretch: Stretch) {
        if self.job.is_some() {
            self.hidden = false;
            return;
        }
        let name =
            crate::clip::file_name(&stretch.folder, &stretch.input, stretch.from, stretch.to);
        self.path = name.to_string_lossy().into_owned();
        self.replace = None;
        self.stretch = Some(stretch);
        self.hidden = false;
        self.error = None;
    }

    /// Show the window again while a stretch is being saved; false when
    /// none is.
    pub fn reveal(&mut self) -> bool {
        if self.job.is_none() {
            return false;
        }
        self.hidden = false;
        true
    }

    /// The window is shown (it holds the player's keys).
    pub fn is_open(&self) -> bool {
        self.stretch.is_some() && !self.hidden
    }

    /// The file chosen in the Save dialog, which asked before replacing one.
    pub fn set_path(&mut self, path: &Path) {
        self.path = path.to_string_lossy().into_owned();
        self.replace = Some(path.to_path_buf());
    }

    /// Stop a saving under way and let its thread delete the file half
    /// written (the program closes).
    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel_and_wait(Duration::from_secs(3));
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, e: &mut Encode) -> Option<Action> {
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
                Err(Failure::Mpv(why)) => {
                    log::warn!("cannot save the stretch: {why}");
                    let hint = if e.nvenc {
                        tr!(
                            " NVENC needs an NVIDIA card and its driver.",
                            " Для NVENC нужна видеокарта NVIDIA с драйвером."
                        )
                    } else {
                        ""
                    };
                    let text = tr!(
                        format!("Cannot save it: {why}.{hint}"),
                        format!("Не удалось сохранить: {why}.{hint}")
                    );
                    if self.hidden {
                        self.stretch = None;
                        return Some(Action::Failed(text));
                    }
                    self.error = Some(text);
                }
            }
        }
        if self.hidden {
            return None;
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
                ui.horizontal(|ui| {
                    if ui.button(tr!("Cancel", "Отмена")).clicked() {
                        job.cancel();
                    }
                    let hide = ui.button(tr!("Hide", "Скрыть")).on_hover_text(tr!(
                        "The saving goes on; the end is shown on the screen",
                        "Сохранение продолжится; о его окончании скажет сообщение на экране"
                    ));
                    if hide.clicked() {
                        close = true;
                    }
                });
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
                    ui.label(tr!("Quality", "Качество")).on_hover_text(tr!(
                        "x264's and x265's CRF, NVENC's CQ: lower is better and larger",
                        "CRF у x264 и x265, CQ у NVENC: меньше — лучше и крупнее файл"
                    ));
                    ui.add(
                        egui::Slider::new(&mut e.quality, QUALITY_RANGE)
                            .text(tr!("lower is better", "меньше — лучше")),
                    );
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
            let output = crate::clip::output_path(&self.path, &stretch.folder);
            match self.check(&output, stretch) {
                Err(why) => self.error = Some(why),
                Ok(()) => {
                    let request = Request {
                        input: stretch.input.clone(),
                        from: stretch.from,
                        to: stretch.to,
                        output,
                        encode: *e,
                        audio: stretch.audio,
                        channels: stretch.channels,
                        subs: stretch.subs.clone(),
                        sub_style: stretch.sub_style.clone(),
                        audio_delay: stretch.audio_delay,
                        sub_delay: stretch.sub_delay,
                        deinterlace: stretch.deinterlace,
                    };
                    let wake = ctx.clone();
                    self.error = None;
                    match Job::start(request, move || wake.request_repaint()) {
                        Ok(job) => self.job = Some(job),
                        Err(why) => self.error = Some(why),
                    }
                }
            }
        }
        if close || modal.should_close() {
            if self.job.is_some() {
                // The saving goes on behind.
                self.hidden = true;
            } else {
                self.stretch = None;
            }
        }
        action
    }

    /// Whether the stretch may go to `output`: not the file playing, not
    /// over another file unless the Save dialog asked, in a folder that is
    /// there or can be made.
    fn check(&self, output: &Path, stretch: &Stretch) -> Result<(), String> {
        // Nor the file written meanwhile (Name.part.mp4 for Name.mp4).
        if !crate::playlist::is_url(&stretch.input)
            && (crate::clip::same_file(output, &stretch.input)
                || crate::clip::same_file(&crate::clip::part_of(output), &stretch.input))
        {
            return Err(tr!(
                "This is the file playing; choose another name.",
                "Это воспроизводимый файл; выберите другое имя."
            )
            .to_owned());
        }
        let replace = self
            .replace
            .as_deref()
            .is_some_and(|r| crate::clip::same_file(r, output));
        if output.exists() && !replace {
            return Err(tr!(
                "A file of this name is there; choose another name, or replace it through …",
                "Файл с таким именем уже есть; выберите другое имя или замените его через …"
            )
            .to_owned());
        }
        if let Some(folder) = output.parent()
            && let Err(e) = std::fs::create_dir_all(folder)
        {
            return Err(tr!(
                format!("Cannot make the folder {}: {e}", folder.display()),
                format!("Не удалось создать папку {}: {e}", folder.display())
            ));
        }
        Ok(())
    }
}
