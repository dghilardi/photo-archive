use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub device_path: PathBuf,
    pub partition_id: String,
}

#[derive(Clone, Debug)]
pub struct MountedPartitionInfo {
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub info: PartitionInfo,
}

impl Display for MountedPartitionInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\t{}",
            self.info.partition_id,
            self.mount_point
                .as_os_str()
                .to_str()
                .map(ToString::to_string)
                .unwrap_or_default()
        )
    }
}

pub (super) struct ProcMountEntry {
    pub device: String,
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub mode: String,
    pub dummy: Vec<String>,
}