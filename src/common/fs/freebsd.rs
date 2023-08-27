use anyhow::bail;
use crate::common::fs::model::MountedPartitionInfo;

pub fn list_mounted_partitions() -> Result<Vec<MountedPartitionInfo>, std::io::Error> {
    eprintln!("!! partitions scan not yet implemented");
    Ok(Vec::new())
}

pub fn partition_by_id(partition_id: &str) -> anyhow::Result<MountedPartitionInfo> {
    eprintln!("!! partitions scan not yet implemented");
    bail!("no partition found")
}