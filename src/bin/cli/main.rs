use std::ffi::OsStr;
use std::fs::create_dir_all;
use std::path::Path;
use anyhow::{anyhow, Context};
use clap::Parser;
use inquire::{Select, Text};
use photo_archive::archive::sync::{synchronize_source, SyncOpts, SyncSource};

use photo_archive::common::fs::{list_mounted_partitions, partition_by_id};

use crate::args::{ImportSourceCliArgs, PhotoArchiveArgs, PhotoArchiveCommand};

mod args;

pub fn main() {
    let args: PhotoArchiveArgs = PhotoArchiveArgs::parse();

    match args.subcommand {
        PhotoArchiveCommand::ListSources => fetch_and_print_sources(),
        PhotoArchiveCommand::ImportSource(args) => import_source(args).expect("Error importing source"),
        PhotoArchiveCommand::SyncSource { .. } => todo!()
    }
}

fn fetch_and_print_sources() {
    let partitions = list_mounted_partitions()
        .expect("Error reading partitions");

    for partition in partitions {
        println!("{}\t{}", partition.info.partition_id, partition.mount_point.to_str().unwrap());
    }
}

fn import_source(args: ImportSourceCliArgs) -> anyhow::Result<()> {
    if !args.target.exists() {
        create_dir_all(&args.target)
            .expect("Error during target dir creation");
    } else if !args.target.is_dir() {
        panic!("Target path is not a directory")
    }

    let source_part = args.source_id
        .map(|source_id| partition_by_id(&source_id).context("Error mapping source_id"))
        .unwrap_or_else(|| {
            let available_partitions = list_mounted_partitions()?;

            Select::new("Choose the source to scan", available_partitions)
                .prompt()
                .context("Error reading source_id")
        })?;

    let source_name = args.source_name.ok_or(anyhow!("unreachable")).or_else(|_| {
        let mut reader = Text::new("Insert a name for the new source");
        reader = if let Some(default_name) = source_part.mount_point.file_name().and_then(OsStr::to_str) {
            reader.with_initial_value(default_name)
        } else {
            reader
        };
        reader.prompt()
    })?;

    let source_group = args.source_group.ok_or(anyhow!("unreachable")).or_else(|_|
        Text::new("Insert a group name for the new source")
            .with_initial_value("ROOT")
            .prompt()
    )?;

    synchronize_source(SyncOpts {
        source: SyncSource::New {
            id: source_part.info.partition_id,
            name: source_name,
            group: source_group,
            tags: vec![],
        },
    }, &args.target)
}