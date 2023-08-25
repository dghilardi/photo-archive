use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use chrono::{Datelike, DateTime, NaiveDateTime, Utc};
use crate::archive::sync::CASTAGNOLI;

pub struct ArchivedPhotoPaths {
    pub date_path: PathBuf,
    pub img_path: PathBuf,
    pub link_dir_path: PathBuf,
    pub link_file_path: PathBuf,
}

pub fn build_paths(
    partition_crc: u32,
    target_base_dir: &Path,
    source_relative_path: &Path,
    photo_timestamp: Option<&NaiveDateTime>,
) -> anyhow::Result<ArchivedPhotoPaths> {
    let date_path = if let Some(datetime) = photo_timestamp {
        target_base_dir
            .join(datetime.year().to_string())
            .join(datetime.format("%m.%d").to_string())
    } else {
        target_base_dir.join("no-date")
    };

    let img_path = date_path.join("img");

    let source_dir = source_relative_path.parent().expect("No source dir found");
    let link_dir_path = date_path.join(format!(
        "{:08X}.{:08X}.{}",
        partition_crc,
        CASTAGNOLI.checksum(source_dir.as_os_str().as_bytes()),
        source_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("Error extracting parent dir"),
    ));
    let link_file_path = link_dir_path.join(source_relative_path.file_name().expect("Error extracting filename"));

    Ok(ArchivedPhotoPaths {
        date_path,
        img_path,
        link_dir_path,
        link_file_path,
    })
}

pub fn build_filename(
    photo_ts: Option<&NaiveDateTime>,
    file_ts: SystemTime,
    crc: u32,
) -> anyhow::Result<String> {
    let file_name = if let Some(datetime) = photo_ts {
        format!(
            "{}_{:08X}.jpg",
            datetime.format("%H%M%S"),
            crc,
        )
    } else {
        format!(
            "{}_{:08X}.jpg",
            DateTime::<Utc>::from(file_ts).format("%Y%m%d-%H%M%S"),
            crc,
        )
    };

    Ok(file_name)
}