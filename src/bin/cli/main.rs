use clap::Parser;

use photo_archive::common::fs::list_mounted_partitions;

use crate::args::{PhotoArchiveArgs, PhotoArchiveCommand};

mod args;

pub fn main() {
    let args: PhotoArchiveArgs = PhotoArchiveArgs::parse();

    match args.subcommand {
        PhotoArchiveCommand::ListSources => fetch_and_print_sources(),
        PhotoArchiveCommand::ImportSource { .. } => {}
    }
}

fn fetch_and_print_sources() {
    let partitions = list_mounted_partitions()
        .expect("Error reading partitions");

    for partition in partitions {
        println!("{}\t{}", partition.info.partition_id, partition.mount_point.to_str().unwrap());
    }
}