use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crossbeam_channel::Sender;
use fun_time::fun_time;
use humansize::{BINARY, format_size};

use crate::common::consts::IMAGE_RS_SIMILAR_IMAGES_EXTENSIONS;
use crate::common::model::WorkContinueStatus;
use crate::common::progress_data::ProgressData;
use crate::common::tool_data::{CommonData, CommonToolData};
use crate::common::traits::{AllTraits, DebugPrint, DeletingItems, FixingItems, PrintResults, Search};
use crate::tools::image_optimizer::{ImageOptimizer, ImageOptimizerParams, Info};

impl AllTraits for ImageOptimizer {}

impl DeletingItems for ImageOptimizer {
    #[fun_time(message = "delete_files", level = "debug")]
    fn delete_files(&mut self, _stop_flag: &Arc<AtomicBool>, _progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        unreachable!("ImageOptimizer does not support deleting files");
    }
}

impl FixingItems for ImageOptimizer {
    type FixParams = ImageOptimizerParams;
    #[fun_time(message = "fix_items", level = "debug")]
    fn fix_items(&mut self, stop_flag: &Arc<AtomicBool>, _progress_sender: Option<&Sender<ProgressData>>, fix_params: Self::FixParams) {
        self.params = fix_params;
        self.optimize_files(stop_flag);
    }
}

impl DebugPrint for ImageOptimizer {
    #[expect(clippy::print_stdout)]
    fn debug_print(&self) {
        if !cfg!(debug_assertions) || cfg!(test) {
            return;
        }

        println!("### INDIVIDUAL DEBUG PRINT ###");
        println!("Info: {:?}", self.information);
        println!("Target format: {:?}", self.params.target_format);
        println!("Quality: {}", self.params.quality);
        println!("Image entries to optimize: {}", self.result_entries.len());
        self.debug_print_common();
        println!("-----------------------------------------");
    }
}

impl PrintResults for ImageOptimizer {
    fn write_results<T: Write>(&self, writer: &mut T) -> std::io::Result<()> {
        self.write_base_search_paths(writer)?;

        writeln!(writer)?;

        let total_entries = self.result_entries.len();
        let failed_entries = self.result_entries.iter().filter(|e| e.error.is_some()).count();

        writeln!(writer, "Total files found: {total_entries}")?;
        writeln!(writer, "Failed to analyze: {failed_entries}")?;
        writeln!(writer)?;

        for entry in &self.result_entries {
            if entry.error.is_none() {
                writeln!(
                    writer,
                    "\"{}\" - Format: {} - Dimensions: {}x{} - Size: {}",
                    entry.path.to_string_lossy(),
                    entry.format,
                    entry.width,
                    entry.height,
                    format_size(entry.size, BINARY)
                )?;
            }
        }

        Ok(())
    }

    fn save_results_to_file_as_json(&self, file_name: &str, pretty_print: bool) -> std::io::Result<()> {
        self.save_results_to_file_as_json_internal(file_name, &self.result_entries, pretty_print)
    }
}

impl CommonData for ImageOptimizer {
    type Info = Info;
    type Parameters = ImageOptimizerParams;

    fn get_information(&self) -> Self::Info {
        self.information
    }
    fn get_params(&self) -> Self::Parameters {
        self.params.clone()
    }
    fn get_cd(&self) -> &CommonToolData {
        &self.common_data
    }
    fn get_cd_mut(&mut self) -> &mut CommonToolData {
        &mut self.common_data
    }
    fn found_any_items(&self) -> bool {
        self.information.number_of_images_to_optimize > 0
    }
}

impl Search for ImageOptimizer {
    #[fun_time(message = "optimize_images", level = "info")]
    fn search(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) {
        let _start_time = Instant::now();

        let () = (|| {
            if self.prepare_items(Some(IMAGE_RS_SIMILAR_IMAGES_EXTENSIONS)).is_err() {
                return;
            }
            if self.scan_files(stop_flag, progress_sender) == WorkContinueStatus::Stop {
                self.common_data.stopped_search = true;
                return;
            }
            if self.check_files(stop_flag, progress_sender) == WorkContinueStatus::Stop {
                self.common_data.stopped_search = true;
            }
        })();
    }
}
