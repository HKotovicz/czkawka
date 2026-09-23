use std::mem;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crossbeam_channel::Sender;
use fun_time::fun_time;
use image::ImageFormat;
use log::info;
use rayon::prelude::*;

use crate::common::dir_traversal::{DirTraversalBuilder, DirTraversalResult};
use crate::common::model::WorkContinueStatus;
use crate::common::progress_data::ProgressData;
use crate::common::progress_stop_handler::check_if_stop_received;
use crate::flc;
use crate::tools::image_optimizer::{ImageOptimizer, ImageOptimizerEntry, ImageOptimizerParams, ImageTargetFormat};

impl ImageOptimizer {
    #[fun_time(message = "scan_files", level = "debug")]
    pub(crate) fn scan_files(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        let result = DirTraversalBuilder::new()
            .group_by(|_fe| ())
            .stop_flag(stop_flag)
            .progress_sender(progress_sender)
            .common_data(&self.common_data)
            .build()
            .run();

        match result {
            DirTraversalResult::SuccessFiles { grouped_file_entries, warnings } => {
                self.test_entries = grouped_file_entries
                    .into_values()
                    .flatten()
                    .map(|fe| {
                        let entry = ImageOptimizerEntry {
                            path: fe.path.clone(),
                            size: fe.size,
                            modified_date: fe.modified_date,
                            error: None,
                            width: 0,
                            height: 0,
                            format: String::new(),
                        };
                        (fe.path.to_string_lossy().to_string(), entry)
                    })
                    .collect();
                info!("Found {} image files to check", self.test_entries.len());
                self.common_data.text_messages.warnings.extend(warnings);
                WorkContinueStatus::Continue
            }
            DirTraversalResult::Stopped => WorkContinueStatus::Stop,
        }
    }

    #[fun_time(message = "check_files", level = "debug")]
    pub(crate) fn check_files(&mut self, stop_flag: &Arc<AtomicBool>, _progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        if self.test_entries.is_empty() {
            return WorkContinueStatus::Continue;
        }

        let mut entries: Vec<ImageOptimizerEntry> = mem::take(&mut self.test_entries)
            .into_par_iter()
            .map(|(_path, entry)| {
                if check_if_stop_received(stop_flag) {
                    return None;
                }
                Some(check_image(entry))
            })
            .while_some()
            .collect();

        self.common_data.text_messages.warnings.extend(entries.iter().filter_map(|e| e.error.as_ref()).cloned());
        entries.retain(|e| e.error.is_none());

        self.result_entries = entries;
        self.information.number_of_images_to_optimize = self.result_entries.len();

        WorkContinueStatus::Continue
    }

    #[fun_time(message = "optimize_files", level = "debug")]
    pub(crate) fn optimize_files(&mut self, stop_flag: &Arc<AtomicBool>) {
        let params = self.params.clone();
        let optimize_warnings: Vec<_> = mem::take(&mut self.result_entries)
            .into_par_iter()
            .map(|entry| {
                if check_if_stop_received(stop_flag) {
                    return None;
                }
                match optimize_single_image(&entry.path, entry.size, &params) {
                    Ok(()) => Some(None),
                    Err(e) => Some(Some(flc!("core_failed_to_optimize_image", file = entry.path.to_string_lossy(), reason = e))),
                }
            })
            .while_some()
            .flatten()
            .collect();

        self.common_data.text_messages.warnings.extend(optimize_warnings);
    }
}

fn check_image(mut entry: ImageOptimizerEntry) -> ImageOptimizerEntry {
    let reader = match std::fs::File::open(&entry.path) {
        Ok(f) => std::io::BufReader::new(f),
        Err(e) => {
            entry.error = Some(flc!("core_failed_to_open_image", file = entry.path.to_string_lossy(), reason = e.to_string()));
            return entry;
        }
    };

    let reader = match image::ImageReader::new(reader).with_guessed_format() {
        Ok(r) => r,
        Err(e) => {
            entry.error = Some(flc!("core_failed_to_read_image", file = entry.path.to_string_lossy(), reason = e.to_string()));
            return entry;
        }
    };

    let Some(format) = reader.format() else {
        entry.error = Some(flc!("core_unknown_image_format", file = entry.path.to_string_lossy()));
        return entry;
    };

    entry.format = format!("{format:?}").to_lowercase();

    match reader.decode() {
        Ok(img) => {
            entry.width = img.width();
            entry.height = img.height();
        }
        Err(e) => {
            entry.error = Some(flc!("core_failed_to_decode_image", file = entry.path.to_string_lossy(), reason = e.to_string()));
        }
    }

    entry
}

pub fn optimize_single_image(input_path: &Path, _original_size: u64, params: &ImageOptimizerParams) -> Result<(), String> {
    let img = image::open(input_path).map_err(|e| flc!("core_failed_to_open_image", file = input_path.to_string_lossy(), reason = e.to_string()))?;

    let ext = if params.target_format == ImageTargetFormat::Same {
        input_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_string()
    } else {
        params.target_format.extension().to_string()
    };
    let output_path = input_path.with_extension(format!("czkawka_optimized.{ext}"));

    let output_format = match params.target_format {
        ImageTargetFormat::Same => match input_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_lowercase().as_str() {
            "png" | "apng" => ImageFormat::Png,
            "webp" => ImageFormat::WebP,
            _ => ImageFormat::Jpeg,
        },
        ImageTargetFormat::Jpeg => ImageFormat::Jpeg,
        ImageTargetFormat::Png => ImageFormat::Png,
        ImageTargetFormat::Webp => ImageFormat::WebP,
    };

    // JPEG/WebP get explicit quality control; PNG uses save_with_format.
    if output_format == ImageFormat::Jpeg {
        let mut out_file = std::fs::File::create(&output_path).map_err(|e| flc!("core_failed_to_create_output", file = format!("{:?}", output_path), reason = e.to_string()))?;
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out_file, params.quality);
        encoder
            .encode(img.as_bytes(), img.width(), img.height(), img.color().into())
            .map_err(|e| flc!("core_failed_to_encode_image", file = format!("{:?}", output_path), reason = e.to_string()))?;
    } else if output_format == ImageFormat::WebP {
        // Force RGBA8 — `webp::Encoder::from_image` only accepts Rgb8/Rgba8 variants.
        let rgba = img.to_rgba8();
        let encoder = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
        let encoded = encoder.encode(params.quality as f32);
        std::fs::write(&output_path, &*encoded).map_err(|e| flc!("core_failed_to_encode_image", file = format!("{:?}", output_path), reason = e.to_string()))?;
    } else {
        img.save_with_format(&output_path, output_format)
            .map_err(|e| flc!("core_failed_to_encode_image", file = format!("{:?}", output_path), reason = e.to_string()))?;
    }

    if params.overwrite_original {
        if params.target_format != ImageTargetFormat::Same {
            // Converting to a different format — replace the original file
            // with the output under the new extension (e.g. photo.jpg → photo.webp).
            // Copy metadata first, before removing the original.
            if params.preserve_metadata {
                copy_exif_metadata(input_path, &output_path);
            }
            let final_path = input_path.with_extension(&ext);
            // Best-effort removal; if it fails the rename below surfaces the error.
            let _ = std::fs::remove_file(input_path);
            std::fs::rename(&output_path, &final_path).map_err(|e| flc!("core_failed_to_replace_original", file = final_path.to_string_lossy(), reason = e.to_string()))?;
        } else {
            std::fs::rename(&output_path, input_path).map_err(|e| flc!("core_failed_to_replace_original", file = input_path.to_string_lossy(), reason = e.to_string()))?;
            if params.preserve_metadata {
                copy_exif_metadata(input_path, input_path);
            }
        }
    } else if params.preserve_metadata {
        copy_exif_metadata(input_path, &output_path);
    }

    Ok(())
}

/// Copy EXIF and other recognized metadata tags from `source` to `dest`.
/// Uses `little_exif` to read all supported tags and embed them in the output.
fn copy_exif_metadata(source: &Path, dest: &Path) {
    let Ok(metadata) = little_exif::metadata::Metadata::new_from_path(source) else {
        return;
    };
    let Ok(mut file_data) = std::fs::read(dest) else {
        return;
    };
    let mut cursor = std::io::Cursor::new(&file_data);
    let Some(ext) = little_exif::filetype::FileExtension::auto_detect(&mut cursor) else {
        return;
    };
    if metadata.write_to_vec(&mut file_data, ext).is_ok() {
        let _ = std::fs::write(dest, &file_data);
    }
}
