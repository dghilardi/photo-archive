use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Error};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub device_path: PathBuf,
    pub partition_id: String,
}

fn partitions_info_lookup() -> Result<HashMap<PathBuf, PartitionInfo>, std::io::Error> {
    let mut result = std::fs::read_dir("/dev/disk/by-uuid")?
        .filter_map(|path_res| path_res.ok())
        .filter_map(|dir_entry| {
            let device_path = std::fs::read_link(dir_entry.path())
                .map(|rel| dir_entry.path().parent().unwrap().join(rel))
                .and_then(std::fs::canonicalize)
                .ok()?;

            Some((dir_entry.path(), PartitionInfo {
                device_path,
                partition_id: String::from(dir_entry.file_name().to_str()?),
            }))
        })
        .flat_map(|(path, info)| [(info.device_path.clone(), info.clone()), (path, info)])
        .collect::<HashMap<_, _>>();

    let mapped_devices = std::fs::read_dir("/dev/mapper")?
        .filter_map(|path_res| path_res.ok())
        .filter_map(|dir_entry| {
            let device_path = std::fs::read_link(dir_entry.path())
                .map(|rel| dir_entry.path().parent().unwrap().join(rel))
                .and_then(std::fs::canonicalize)
                .ok()?;

            let current_entry = result.get(&device_path)?;

            Some((dir_entry.path(), current_entry.clone()))
        })
        .collect::<Vec<_>>();

    result.extend(mapped_devices);

    Ok(result)
}

#[derive(Debug)]
pub struct MountedPartitionInfo {
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub info: PartitionInfo,
}

pub fn list_mounted_partitions() -> Result<Vec<MountedPartitionInfo>, Error> {
    let lookup = partitions_info_lookup()?;

    let file = File::open("/proc/mounts")?;
    let mut vdisks: Vec<MountedPartitionInfo> = Vec::new();
    let mut file = BufReader::with_capacity(6144, file);

    let mut line = String::with_capacity(512);
    while file.read_line(&mut line)? != 0 {
        let mut fields = line.split_whitespace();
        let name = fields.next().unwrap();
        let path = fields.next().unwrap();
        let fs_type = fields.next().unwrap();

        if is_supported_fs(fs_type) {
            let mount_point = PathBuf::from(path);

            if let Some(partition_info) = lookup.get(&PathBuf::from(name)) {
                vdisks.push(MountedPartitionInfo {
                    mount_point,
                    fs_type: String::from(fs_type),
                    info: partition_info.clone(),
                });
            }

        }
        line.clear()
    }

    Ok(vdisks)
}

fn is_supported_fs(fs_type: &str) -> bool {
    ["vfat", "ntfs3"].contains(&fs_type)
}