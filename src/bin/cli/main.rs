use std::fs::create_dir_all;
use std::path::Path;
use clap::Parser;
use photo_archive::archive::sync::synchronize_source;

use photo_archive::common::fs::{list_mounted_partitions, partition_by_id};

use crate::args::{PhotoArchiveArgs, PhotoArchiveCommand};

mod args;

pub fn main() {
    let args: PhotoArchiveArgs = PhotoArchiveArgs::parse();

    match args.subcommand {
        PhotoArchiveCommand::ListSources => fetch_and_print_sources(),
        PhotoArchiveCommand::ImportSource { source_id, target } => import_source(&source_id, target.as_path()),
    }
}

fn fetch_and_print_sources() {
    let partitions = list_mounted_partitions()
        .expect("Error reading partitions");

    for partition in partitions {
        println!("{}\t{}", partition.info.partition_id, partition.mount_point.to_str().unwrap());
    }
}

fn import_source(source_id: &str, target: &Path) {
    let source = partition_by_id(source_id)
        .expect("Cannot load source");

    if !target.exists() {
        create_dir_all(target)
            .expect("Error during target dir creation");
    } else if !target.is_dir() {
        panic!("Target path is not a directory")
    }

    synchronize_source(source.mount_point.as_path(), target)
        .expect("Error during synchronization")
}