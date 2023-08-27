use std::path::Path;
use anyhow::bail;
use serde::Deserialize;
use crate::common::fs::model::{MountedPartitionInfo, PartitionInfo};

#[derive(Debug, Deserialize)]
struct SourceMeta {
    source_id: String,
}

pub fn partition_by_path(path: &Path) -> anyhow::Result<MountedPartitionInfo> {
    let source_meta_file_path = path.join(".photo-archive-source");
    if source_meta_file_path.is_file() {
        let meta: SourceMeta = toml::from_str(&std::fs::read_to_string(&source_meta_file_path)?)?;
        Ok(MountedPartitionInfo {
            mount_point: path.to_path_buf(),
            fs_type: String::from("-"),
            info: PartitionInfo {
                device_path: source_meta_file_path,
                partition_id: meta.source_id,
            },
        })
    } else {
        bail!("Could not find .photo-archive-source file in {path:?}")
    }
}