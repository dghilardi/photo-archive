use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDateTime};
use exif::Exif;
use polars::df;
use polars::export::ahash::HashMap;
use polars::io::SerWriter;
use polars::prelude::{ParquetWriter, NamedFrom, JsonWriter};
use serde::Serialize;

pub struct PhotoArchiveRow {
    pub timestamp: NaiveDateTime,
    pub source_id: String,
    pub source_path: PathBuf,
    pub exif: Exif,
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
            timestamp: row.timestamp.timestamp(),
            source: row.source_id,
            path: row.source_path.as_os_str().to_str().map(ToString::to_string).unwrap_or_default(),
            exif: row.exif.fields().map(|f| (format!("{}:{}", f.tag, f.ifd_num), f.display_value().to_string())).collect::<HashMap<String, String>>(),
        }).unwrap();

        let mut file = std::fs::File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(self.base_dir.join(row.timestamp.year().to_string()).join("index.json")).unwrap();

        file.write(frame.as_bytes()).unwrap();
        file.write(b"\n").unwrap();
    }
}

#[derive(Serialize)]
struct PhotoArchiveJsonRow {
    timestamp: i64,
    source: String,
    path: String,
    exif: HashMap<String, String>,
}

