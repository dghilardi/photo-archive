use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use exif::Exif;
use serde::{Deserialize, Serialize};

pub struct PhotoArchiveRow {
    pub photo_ts: Option<NaiveDateTime>,
    pub file_ts: SystemTime,
    pub source_id: String,
    pub source_path: PathBuf,
    pub exif: Option<Exif>,
    pub size: u64,
    pub height: u32,
    pub width: u32,
    pub digest: u32,
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
            file_ts: row.file_ts.duration_since(SystemTime::UNIX_EPOCH)
                .expect("Ts is before unix epoch")
                .as_secs(),
            source: row.source_id,
            path: row.source_path.as_os_str().to_str().map(ToString::to_string).unwrap_or_default(),
            exif: row.exif
                .map(|exif| Vec::from(exif.buf()))
                .unwrap_or_default(),
            size: row.size,
            height: row.height,
            width: row.width,
            crc: row.digest,
        }).unwrap();

        let mut file = std::fs::File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(self.base_dir.join(row.photo_ts.map(|ts| ts.year().to_string()).unwrap_or_else(|| String::from("no-date"))).join("index.json")).unwrap();

        file.write(frame.as_bytes()).unwrap();
        file.write(b"\n").unwrap();
    }

    fn indexes_list(&self) -> anyhow::Result<impl Iterator<Item=PathBuf>> {
        let iter = fs::read_dir(&self.base_dir)?
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| Some(entry.path().join("index.json")).filter(|p| p.is_file()));
        Ok(iter)
    }

    pub fn retain(&self, mut f: impl FnMut(&PhotoArchiveJsonRow) -> bool) -> anyhow::Result<()> {
        for index_path in self.indexes_list()? {
            let file = File::open(&index_path)?;
            let reader = BufReader::new(file);

            let temp_path = PathBuf::from(format!("/tmp/index.{}.{}.json", index_path.parent().unwrap().file_name().and_then(|name| name.to_str()).unwrap_or("-"), Utc::now().format("%Y%m%d-%H%M%S")));
            let temp_file = File::create(&temp_path)?;
            let mut writer = BufWriter::new(temp_file);

            for res_line in reader.lines() {
                let line = res_line?;
                let row = serde_json::from_str::<PhotoArchiveJsonRow>(&line)?;
                if f(&row) {
                    writer.write(line.as_bytes())?;
                }
            }
            writer.flush()?;
            drop(writer);

            std::fs::rename(&temp_path, &index_path)?;
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct PhotoArchiveJsonRow {
    #[serde(rename = "ts")]
    timestamp: Option<i64>,
    #[serde(rename = "fts")]
    file_ts: u64,
    #[serde(rename = "src")]
    source: String,
    #[serde(rename = "pth")]
    path: String,
    #[serde(rename = "exf", with = "base64")]
    exif: Vec<u8>,
    #[serde(rename = "siz")]
    size: u64,
    #[serde(rename = "hgh")]
    height: u32,
    #[serde(rename = "wdt")]
    width: u32,
    crc: u32,
}

impl PhotoArchiveJsonRow {
    pub fn timestamp(&self) -> Option<NaiveDateTime> {
        self.timestamp.and_then(|ts| NaiveDateTime::from_timestamp_opt(ts, 0))
    }

    pub fn file_timestamp(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH.add(Duration::from_secs(self.file_ts as u64))
    }

    pub fn source_id(&self) -> &str {
        &self.source
    }

    pub fn source_path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn digest(&self) -> u32 {
        self.crc
    }
}

mod base64 {
    use serde::{Serialize, Deserialize};
    use serde::{Deserializer, Serializer};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let base64 = STANDARD.encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        STANDARD.decode(base64.as_bytes())
            .map_err(|e| serde::de::Error::custom(e))
    }
}

