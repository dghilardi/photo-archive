use std::ffi::OsStr;
use std::fs::create_dir_all;
use anyhow::{anyhow, Context};
use clap::Parser;
use inquire::{Select, Text};
use photo_archive::archive::sync::{SynchronizationEvent, synchronize_source, SyncOpts, SyncSource};

use photo_archive::common::fs::{list_mounted_partitions, partition_by_id};
use photo_archive::repository::sources::SourcesRepo;

use crate::args::{ImportSourceCliArgs, PhotoArchiveArgs, PhotoArchiveCommand, RemoveSourceCliArgs, SyncSourceCliArgs};

mod args;

pub fn main() {
    let args: PhotoArchiveArgs = PhotoArchiveArgs::parse();

    let out = match args.subcommand {
        PhotoArchiveCommand::ListSources => fetch_and_print_sources(),
        PhotoArchiveCommand::ImportSource(args) => import_source(args),
        PhotoArchiveCommand::SyncSource(args) => sync_source(args),
        PhotoArchiveCommand::RemoveSource(args) => remove_source(args),
    };

    if let Err(err) = out {
        eprintln!("Error - {err}");
    }
}

fn fetch_and_print_sources() -> anyhow::Result<()> {
    let partitions = list_mounted_partitions()
        .context("Error reading partitions")?;

    for partition in partitions {
        println!("{partition}");
    }
    Ok(())
}

fn import_source(args: ImportSourceCliArgs) -> anyhow::Result<()> {
    if !args.target.exists() {
        create_dir_all(&args.target)
            .context("Error during target dir creation")?;
    } else if !args.target.is_dir() {
        anyhow::bail!("Target path is not a directory")
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

    let task = synchronize_source(SyncOpts {
        count_images: true,
        source: SyncSource::New {
            id: source_part.info.partition_id,
            name: source_name,
            group: source_group,
            tags: vec![],
        },
    }, &args.target)?;

    let mut total_images = 0;
    let mut processed_images = 0;

    while let Ok(evt) = task.evt_stream().recv() {
        if let SynchronizationEvent::ScanProgress { count } | SynchronizationEvent::ScanCompleted { count } = &evt {
            total_images = *count;
        } else {
            processed_images += 1;
        }
        println!("{processed_images}/{total_images} ({:02.02}%)", (processed_images as f32 / total_images as f32 * 100.0));
        match evt {
            SynchronizationEvent::Stored { src, dst, generated, partial } => println!("[STR] {src:?} -> {dst:?} [gen: {generated}; par: {partial}]"),
            SynchronizationEvent::Skipped { src, existing } => println!("[SKP] {src:?} (existing: {existing:?})"),
            SynchronizationEvent::Errored { src, cause } => println!("[ERR] {src:?} - {cause}"),
            SynchronizationEvent::Ignored { src, cause } => println!("[IGN] {src:?} - {cause})"),
            SynchronizationEvent::ScanProgress { .. } | SynchronizationEvent::ScanCompleted { .. } => {}
        }
    }

    task.join()?;
    Ok(())
}

fn sync_source(args: SyncSourceCliArgs) -> anyhow::Result<()> {
    if !args.target.exists() {
        create_dir_all(&args.target)
            .context("Error during target dir creation")?;
    } else if !args.target.is_dir() {
        anyhow::bail!("Target path is not a directory")
    }

    let source_part = args.source_id
        .map(|source_id| partition_by_id(&source_id).context("Error mapping source_id"))
        .unwrap_or_else(|| {
            let repo = SourcesRepo::new(args.target.clone());
            let registered_sources = repo.all()?;
            let mut available_partitions = list_mounted_partitions()?;
            available_partitions.retain(|src| registered_sources.iter().any(|reg| reg.id.eq(&src.info.partition_id)));

            if available_partitions.is_empty() {
                anyhow::bail!("None of the registered partitions is currently mounted");
            }

            Select::new("Choose the source to scan", available_partitions)
                .prompt()
                .context("Error reading source_id")
        })?;

    let task = synchronize_source(SyncOpts {
        count_images: true,
        source: SyncSource::Existing {
            id: source_part.info.partition_id,
        },
    }, &args.target)?;

    let mut total_images = 0;
    let mut processed_images = 0;

    while let Ok(evt) = task.evt_stream().recv() {
        if let SynchronizationEvent::ScanProgress { count } | SynchronizationEvent::ScanCompleted { count } = &evt {
            total_images = *count;
        } else {
            processed_images += 1;
        }
        println!("{processed_images}/{total_images} ({:02.02}%)", (processed_images as f32 / total_images as f32 * 100.0));
        match evt {
            SynchronizationEvent::Stored { src, dst, generated, partial } => println!("[STR] {src:?} -> {dst:?} [gen: {generated}; par: {partial}]"),
            SynchronizationEvent::Skipped { src, existing } => println!("[SKP] {src:?} (existing: {existing:?})"),
            SynchronizationEvent::Errored { src, cause } => println!("[ERR] {src:?} - {cause}"),
            SynchronizationEvent::Ignored { src, cause } => println!("[IGN] {src:?} - {cause}"),
            SynchronizationEvent::ScanProgress { .. } | SynchronizationEvent::ScanCompleted { .. } => {}
        }
    }

    task.join()?;
    Ok(())
}

fn remove_source(args: RemoveSourceCliArgs) -> anyhow::Result<()> {
    if !args.target.exists() {
        anyhow::bail!("Target path does not exists")
    } else if !args.target.is_dir() {
        anyhow::bail!("Target path is not a directory")
    }
    let repo = SourcesRepo::new(args.target.clone());

    let source_part = args.source_id
        .map(|source_id| {
            repo.find_by_id(&source_id)
                .transpose()
                .ok_or_else(|| anyhow!("Could not find registered source with id {source_id}"))?
        })
        .unwrap_or_else(|| {
            let registered_sources = repo.all()?;

            if registered_sources.is_empty() {
                anyhow::bail!("There are no registered sources in the specified archive");
            }

            Select::new("Choose the source to remove", registered_sources)
                .prompt()
                .context("Error reading source_id")
        })?;

    Ok(())
}