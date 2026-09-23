use std::path::MAIN_SEPARATOR;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::Sender;
use czkawka_core::common::progress_data::ProgressData;
use czkawka_core::tools::image_optimizer::{ImageOptimizerParams, ImageTargetFormat};
use slint::{ComponentHandle, Weak};

use crate::model_operations::model_processor::{MessageType, ModelProcessor, ProcessFunction};
use crate::simpler_model::{SimplerSingleMainListModel, ToSimplerVec};
use crate::{Callabler, GuiState, MainWindow, Settings};

pub(crate) fn connect_optimize_image(app: &MainWindow, progress_sender: Sender<ProgressData>, stop_flag: Arc<AtomicBool>) {
    let a = app.as_weak();

    app.global::<Callabler>().on_optimize_image_items(move || {
        let weak_app = a.clone();
        let progress_sender = progress_sender.clone();
        let stop_flag = stop_flag.clone();
        stop_flag.store(false, Ordering::Relaxed);
        let app = a.upgrade().expect("Failed to upgrade app :(");
        let active_tab = app.global::<GuiState>().get_active_tab();

        let settings = app.global::<Settings>();
        let overwrite_files = settings.get_popup_optimize_image_overwrite_files();
        let preserve_metadata = settings.get_popup_optimize_image_preserve_metadata();

        let target_format = settings
            .get_image_optimizer_sub_target_format_value()
            .to_lowercase()
            .parse::<ImageTargetFormat>()
            .unwrap_or(ImageTargetFormat::Same);
        let quality = settings.get_image_optimizer_sub_quality().round() as u8;

        let processor = ModelProcessor::new(active_tab);

        processor.optimize_selected_images(progress_sender, weak_app, stop_flag, target_format, quality, overwrite_files, preserve_metadata);
    });
}

impl ModelProcessor {
    fn optimize_selected_images(
        self,
        progress_sender: Sender<ProgressData>,
        weak_app: Weak<MainWindow>,
        stop_flag: Arc<AtomicBool>,
        target_format: ImageTargetFormat,
        quality: u8,
        overwrite_files: bool,
        preserve_metadata: bool,
    ) {
        let model = self.active_tab.get_tool_model(&weak_app.upgrade().expect("Failed to upgrade app :("));
        let simpler_model = model.to_simpler_enumerated_vec();
        thread::spawn(move || {
            let path_idx = self.active_tab.get_str_path_idx();
            let name_idx = self.active_tab.get_str_name_idx();
            let size_idx = self.active_tab.get_int_size_idx();

            let _stop_flag_clone = stop_flag.clone();
            let optimize_fnc = move |data: &SimplerSingleMainListModel| {
                let full_path = format!("{}{MAIN_SEPARATOR}{}", data.val_str[path_idx], data.val_str[name_idx]);
                let original_size = data.get_size(size_idx);

                let params = ImageOptimizerParams::new(target_format, quality, overwrite_files).with_metadata_preservation(preserve_metadata);

                czkawka_core::tools::image_optimizer::core::optimize_single_image(&std::path::PathBuf::from(&full_path), original_size, &params).map_err(|e| format!("{e}"))
            };

            self.process_and_update_gui_state(
                &weak_app,
                stop_flag,
                &progress_sender,
                simpler_model,
                &ProcessFunction::Simple(Box::new(optimize_fnc)),
                MessageType::OptimizeImage,
                false,
            );
        });
    }
}
