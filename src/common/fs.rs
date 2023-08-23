use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub device_path: PathBuf,
    pub partition_id: String,
}

fn disk_by_uuid_device_path(uuid: &str) -> PathBuf {
    PathBuf::from("/dev/disk/by-uuid").join(uuid)
}

fn partition_info_by_uuid(uuid: &str) -> Result<PartitionInfo, std::io::Error> {
    let device_path = disk_by_uuid_device_path(uuid);
    let device_path = std::fs::read_link(&device_path)
        .map(|rel| device_path.parent().unwrap().join(rel))
        .and_then(std::fs::canonicalize)?;

    Ok(PartitionInfo {
        device_path,
        partition_id: String::from(uuid),
    })
}

fn partitions_by_uuid_lookup() -> Result<HashMap<String, PartitionInfo>, std::io::Error> {
    let result = std::fs::read_dir("/dev/disk/by-uuid")?
        .filter_map(|path_res| path_res.ok())
        .filter_map(|dir_entry| {
            let device_path = std::fs::read_link(dir_entry.path())
                .map(|rel| dir_entry.path().parent().unwrap().join(rel))
                .and_then(std::fs::canonicalize)
                .ok()?;

            let partition_id = String::from(dir_entry.file_name().to_str()?);
            Some((partition_id.clone(), PartitionInfo {
                device_path,
                partition_id,
            }))
        })
        .collect::<HashMap<_, _>>();

    Ok(result)
}

fn partitions_info_lookup() -> Result<HashMap<PathBuf, PartitionInfo>, std::io::Error> {
    let mut result = partitions_by_uuid_lookup()?
        .into_iter()
        .map(|(partition_id, info)| (disk_by_uuid_device_path(&partition_id), info))
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

#[derive(Clone, Debug)]
pub struct MountedPartitionInfo {
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub info: PartitionInfo,
}

impl Display for MountedPartitionInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}", self.info.partition_id, self.mount_point.as_os_str().to_str().map(ToString::to_string).unwrap_or_default())
    }
}

struct ProcMountEntry {
    device: String,
    mount_point: PathBuf,
    fs_type: String,
    mode: String,
    dummy: Vec<String>,
}

fn read_proc_mounts() -> Result<Vec<ProcMountEntry>, std::io::Error> {
    let file = File::open("/proc/mounts")?;
    let mut vdisks: Vec<ProcMountEntry> = Vec::new();
    let mut file = BufReader::with_capacity(6144, file);

    let mut line = String::with_capacity(512);

    while file.read_line(&mut line)? != 0 {
        let mut fields = line.split_whitespace();
        let device = fields.next().unwrap();
        let path = fields.next().unwrap().replace("\\040", " ");
        let fs_type = fields.next().unwrap();
        let mode = fields.next().unwrap();

        vdisks.push(ProcMountEntry {
            device: String::from(device),
            mount_point: PathBuf::from(path),
            fs_type: String::from(fs_type),
            mode: String::from(mode),
            dummy: fields.next()
                .map(|dummy| dummy.split(',').map(ToString::to_string).collect())
                .unwrap_or_default(),
        });
        line.clear();
    }

    Ok(vdisks)
}

pub fn list_mounted_partitions() -> Result<Vec<MountedPartitionInfo>, std::io::Error> {
    let lookup = partitions_info_lookup()?;

    let vdisks = read_proc_mounts()?
        .into_iter()
        .filter(|entry| is_supported_fs(&entry.fs_type))
        .filter_map(|entry| {
            let Some(partition_info) = lookup.get(&PathBuf::from(&entry.device)) else {
                eprintln!("No partition_info found");
                return None;
            };
            Some(MountedPartitionInfo {
                mount_point: entry.mount_point,
                fs_type: entry.fs_type,
                info: partition_info.clone(),
            })
        })
        .collect();

    Ok(vdisks)
}

fn is_supported_fs(fs_type: &str) -> bool {
    ["vfat", "ntfs3", "fuseblk"].contains(&fs_type)
}

pub fn partition_by_id(partition_id: &str) -> Result<MountedPartitionInfo, std::io::Error> {
    let lookup = partitions_info_lookup()?;
    let proc_mounts = read_proc_mounts()?.into_iter()
        .filter(|e| is_supported_fs(&e.fs_type))
        .filter_map(|e| lookup.get(&PathBuf::from(&e.device)).map(|pi| (pi, e)))
        .filter(|(pi, _e)| pi.partition_id.eq(partition_id))
        .map(|(pi, e)| MountedPartitionInfo {
            mount_point: e.mount_point,
            fs_type: e.fs_type,
            info: pi.clone(),
        })
        .collect::<Vec<_>>();

    match &proc_mounts[..] {
        [] => panic!("No partition found"),
        [mpi] => Ok(mpi.clone()),
        [_, ..] => panic!("Multiple partitions with same id"),
    }
}