use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use exif::Exif;
use serde::Serialize;

pub struct PhotoArchiveRow {
    pub photo_ts: Option<NaiveDateTime>,
    pub file_ts: DateTime<Utc>,
    pub source_id: String,
    pub source_path: PathBuf,
    pub exif: Option<Exif>,
    pub size: u64,
    pub height: u32,
    pub width: u32,
}

pub struct PhotoArchiveRecordsStore {
    base_dir: PathBuf,
}

impl PhotoArchiveRecordsStore {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    pub fn write(&self, row: PhotoArchiveRow) {
        let frame = serde_json::to_string(&PhotoArchiveJsonRow {
            timestamp: row.photo_ts.map(|ts| ts.timestamp()),
            file_ts: row.file_ts.naive_local().timestamp(),
            source: row.source_id,
            path: row.source_path.as_os_str().to_str().map(ToString::to_string).unwrap_or_default(),
            exif: row.exif
                .map(|exif| exif.fields().map(|f| (format!("{}:{}", f.tag.number(), f.ifd_num.0), f.display_value().to_string())).collect::<HashMap<String, String>>())
                .unwrap_or_default(),
            size: row.size,
            height: row.height,
            width: row.width,
        }).unwrap();

        let mut file = std::fs::File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(self.base_dir.join(row.photo_ts.map(|ts| ts.year().to_string()).unwrap_or_else(|| String::from("no-date"))).join("index.json")).unwrap();

        file.write(frame.as_bytes()).unwrap();
        file.write(b"\n").unwrap();
    }
}

#[derive(Serialize)]
struct PhotoArchiveJsonRow {
    #[serde(rename="ts")]
    timestamp: Option<i64>,
    #[serde(rename="fts")]
    file_ts: i64,
    #[serde(rename="src")]
    source: String,
    #[serde(rename="pth")]
    path: String,
    #[serde(rename="exf")]
    exif: HashMap<String, String>,
    #[serde(rename="siz")]
    size: u64,
    #[serde(rename="hgh")]
    height: u32,
    #[serde(rename="wdt")]
    width: u32,
}

