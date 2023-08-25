use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::archive::common::{build_filename, build_paths};
use crate::archive::records_store::{PhotoArchiveJsonRow, PhotoArchiveRecordsStore};
use crate::archive::sync::CASTAGNOLI;

pub fn retain_images(target: PathBuf, mut condition: impl FnMut(&PhotoArchiveJsonRow) -> bool) -> anyhow::Result<()> {
    let store = PhotoArchiveRecordsStore::new(&target);

    let mut thumbnail_with_link = HashSet::new();
    let mut thumbnail_to_remove = HashSet::new();

    store.retain(|row| {
        let retain = condition(row);

        let photo_timestamp = row.timestamp();
        let file_timestamp = row.file_timestamp();

        let archive_paths = build_paths(
            CASTAGNOLI.checksum(row.source_id().as_bytes()),
            &target,
            &row.source_path(),
            photo_timestamp.as_ref(),
        ).expect("Error building paths");

        let thumbnail_path = archive_paths.img_path.join(build_filename(
            photo_timestamp.as_ref(),
            file_timestamp,
            row.digest(),
        ).expect("Error building filename"));

        if retain {
            thumbnail_to_remove.remove(&thumbnail_path);
            thumbnail_with_link.insert(thumbnail_path);
        } else {
            if !thumbnail_with_link.contains(&thumbnail_path) {
                thumbnail_to_remove.insert(thumbnail_path);
            }

            if archive_paths.link_file_path.exists() {
                std::fs::remove_file(archive_paths.link_file_path)
                    .expect("Error removing symlink file");
            }

            if archive_paths.link_dir_path.exists() && archive_paths.link_dir_path.read_dir().expect("Error reading dir").next().is_none() {
                std::fs::remove_dir(archive_paths.link_dir_path)
                    .expect("Error removing symlink dir");
            }
        }
        retain
    })?;

    for f in thumbnail_to_remove {
        let remove_out = std::fs::remove_file(&f);
        if let Err(err) = remove_out {
            eprintln!("Error removing file {f:?} - {err}")
        } else {
            println!("Removed file {f:?}");
        }
    }

    Ok(())
}