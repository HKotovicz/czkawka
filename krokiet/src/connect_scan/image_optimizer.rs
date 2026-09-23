use std::rc::Rc;
use std::thread;

use czkawka_core::common::consts::DEFAULT_THREAD_SIZE;
use czkawka_core::common::tool_data::CommonData;
use czkawka_core::common::traits::Search;
use czkawka_core::common::{format_time, split_path};
use czkawka_core::tools::image_optimizer::{ImageOptimizer, ImageOptimizerEntry, ImageOptimizerParams, ImageTargetFormat};
use humansize::{BINARY, format_size};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use crate::common::{MAX_INT_DATA_IMAGE_OPTIMIZER, MAX_STR_DATA_IMAGE_OPTIMIZER, split_u64_into_i32s};
use crate::connect_scan::{MessagesData, ScanData, get_dt_timestamp_string, get_text_messages, insert_data_to_model, reset_selection_at_end, set_common_settings};
use crate::{ActiveTab, GuiState, MainWindow, flk};

pub(crate) fn scan_image_optimizer(a: Weak<MainWindow>, sd: ScanData) {
    let thread_builder = thread::Builder::new().stack_size(DEFAULT_THREAD_SIZE);
    thread_builder
        .spawn(move || {
            let params = ImageOptimizerParams::new(ImageTargetFormat::Same, 80, false);
            let mut tool = ImageOptimizer::new(params);
            set_common_settings(&mut tool, &sd.custom_settings, &sd.stop_flag);
            tool.search(&sd.stop_flag, Some(&sd.progress_sender));

            let vector = tool.get_entries().clone();
            let (critical, messages) = get_text_messages(&tool, &sd.basic_settings);

            let info = tool.get_information();
            let stopped_search = tool.get_stopped_search();
            sd.shared_models.lock().expect("Mutex poisoned").shared_image_optimizer_state = Some(tool);

            let messages_data = MessagesData { critical, messages };

            a.upgrade_in_event_loop(move |app| {
                write_image_optimizer_results(&app, vector, messages_data, info, sd, stopped_search);
            })
        })
        .expect("Cannot start thread - not much we can do here");
}

fn write_image_optimizer_results(
    app: &MainWindow,
    vector: Vec<ImageOptimizerEntry>,
    messages_data: MessagesData,
    info: czkawka_core::tools::image_optimizer::Info,
    sd: ScanData,
    stopped_search: bool,
) {
    let scanning_time_str = format_time(info.scanning_time);
    let items_found = info.number_of_images_to_optimize;

    let items = Rc::new(VecModel::default());
    for entry in vector {
        let (data_model_int, data_model_str) = prepare_data_model_image_optimizer(&entry);
        insert_data_to_model(&items, data_model_str, data_model_int, None);
    }
    app.set_image_optimizer_model(items.into());
    if let Some(critical) = messages_data.critical {
        app.invoke_scan_ended(critical.into());
    } else {
        if !stopped_search && sd.basic_settings.play_audio_on_scan_completion {
            sd.audio_player.play_scan_completed();
        }
        let result_message = flk!("rust_found_image_files", items_found = items_found, time = scanning_time_str);
        if !stopped_search && sd.basic_settings.show_notification_on_scan_completion {
            crate::notification_manager::send_scan_completed_notification("Image Optimizer", &result_message);
        }
        app.invoke_scan_ended(result_message.into());
    }
    app.global::<GuiState>().set_info_text(messages_data.messages.into());
    reset_selection_at_end(app, ActiveTab::ImageOptimizer);
}

fn prepare_data_model_image_optimizer(entry: &ImageOptimizerEntry) -> (ModelRc<i32>, ModelRc<SharedString>) {
    let (directory, file) = split_path(&entry.path);
    let size_str = format_size(entry.size, BINARY);
    let dimensions_str = format!("{}x{}", entry.width, entry.height);

    let data_model_str_arr: [SharedString; MAX_STR_DATA_IMAGE_OPTIMIZER] = [
        size_str.into(),
        file.into(),
        directory.into(),
        entry.format.clone().into(),
        dimensions_str.into(),
        get_dt_timestamp_string(entry.modified_date).into(),
    ];
    let data_model_str = VecModel::from_slice(&data_model_str_arr);

    let modification_split = split_u64_into_i32s(entry.modified_date);
    let size_split = split_u64_into_i32s(entry.size);
    let data_model_int_arr: [i32; MAX_INT_DATA_IMAGE_OPTIMIZER] = [
        modification_split.0,
        modification_split.1,
        size_split.0,
        size_split.1,
        entry.width as i32,
        entry.height as i32,
    ];
    let data_model_int = VecModel::from_slice(&data_model_int_arr);

    (data_model_int, data_model_str)
}
