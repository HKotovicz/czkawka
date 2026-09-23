pub mod core;
pub mod traits;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::common::tool_data::CommonToolData;

/// Target image format for optimization.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImageTargetFormat {
    /// Re-encode in the same format with lower quality / higher compression.
    Same,
    Jpeg,
    Png,
    Webp,
}

impl ImageTargetFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Same => "",
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Same => "Same",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Webp => "WebP",
        }
    }
}

impl std::str::FromStr for ImageTargetFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "same" | "" => Ok(Self::Same),
            "jpeg" | "jpg" => Ok(Self::Jpeg),
            "png" => Ok(Self::Png),
            "webp" => Ok(Self::Webp),
            _ => Err(format!("Unknown image target format: {s}")),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ImageOptimizerParams {
    pub target_format: ImageTargetFormat,
    /// Quality 1–100 (JPEG/WebP: encoder quality; PNG: compression level mapped inversely).
    pub quality: u8,
    pub overwrite_original: bool,
    /// Copy EXIF and other metadata from the source file to the output.
    pub preserve_metadata: bool,
}

impl ImageOptimizerParams {
    pub fn new(target_format: ImageTargetFormat, quality: u8, overwrite_original: bool) -> Self {
        let quality = quality.clamp(1, 100);
        Self { target_format, quality, overwrite_original, preserve_metadata: false }
    }

    pub fn with_metadata_preservation(mut self, preserve: bool) -> Self {
        self.preserve_metadata = preserve;
        self
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Info {
    pub scanning_time: Duration,
    pub number_of_images_to_optimize: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageOptimizerEntry {
    pub path: PathBuf,
    pub size: u64,
    pub modified_date: u64,
    pub error: Option<String>,

    pub width: u32,
    pub height: u32,
    pub format: String,
}

impl crate::common::traits::ResultEntry for ImageOptimizerEntry {
    fn get_path(&self) -> &Path {
        &self.path
    }
    fn get_modified_date(&self) -> u64 {
        self.modified_date
    }
    fn get_size(&self) -> u64 {
        self.size
    }
}

pub struct ImageOptimizer {
    pub(crate) common_data: CommonToolData,
    pub(crate) information: Info,
    pub(crate) test_entries: std::collections::BTreeMap<String, ImageOptimizerEntry>,
    pub(crate) result_entries: Vec<ImageOptimizerEntry>,
    pub(crate) params: ImageOptimizerParams,
}

impl ImageOptimizer {
    pub fn new(params: ImageOptimizerParams) -> Self {
        Self {
            common_data: CommonToolData::new(crate::common::model::ToolType::ImageOptimizer),
            information: Info::default(),
            test_entries: Default::default(),
            result_entries: Vec::new(),
            params,
        }
    }

    pub fn get_entries(&self) -> &Vec<ImageOptimizerEntry> {
        &self.result_entries
    }

    pub fn get_params(&self) -> &ImageOptimizerParams {
        &self.params
    }

    pub const fn get_information(&self) -> Info {
        self.information
    }
}
