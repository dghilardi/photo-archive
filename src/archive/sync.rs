use std::fmt::format;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::Add;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use std::{fs, thread};

use anyhow::{anyhow, Context};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDateTime, Utc};
use crc::{Crc, CRC_32_ISCSI};
use crossbeam::channel::{Receiver, Sender};
use exif::{Exif, Tag};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};
use crate::archive::common::{build_filename, build_paths};

use crate::archive::records_store::{PhotoArchiveRecordsStore, PhotoArchiveRow};
use crate::common::fs::model::MountedPartitionInfo;
use crate::repository::sources::{SourceJsonRow, SourcesRepo};

pub struct SyncOpts {
    pub count_images: bool,
    pub source: SyncSource,
}

pub enum SourceCoordinates {
    Id(String),
    Path(PathBuf),
}

pub enum SyncSource {
    New {
        coord: SourceCoordinates,
        name: String,
        group: String,
        tags: Vec<String>,
    },
    Existing {
        coord: SourceCoordinates,
    },
}

pub enum SynchronizationEvent {
    ScanProgress {
        count: u64,
    },
    ScanCompleted {
        count: u64,
    },
    Stored {
        src: PathBuf,
        dst: PathBuf,
        generated: bool,
        partial: bool,
    },
    Skipped {
        src: PathBuf,
        existing: PathBuf,
    },
    Ignored {
        src: PathBuf,
        cause: String,
    },
    Errored {
        src: PathBuf,
        cause: String,
    },
}

pub struct SyncrhonizationTask {
    events_stream: Receiver<SynchronizationEvent>,
    handlers: Vec<JoinHandle<()>>,
}

impl SyncrhonizationTask {
    pub fn join(self) -> anyhow::Result<()> {
        drop(self.events_stream);
        for handler in self.handlers {
            handler
                .join()
                .map_err(|err| anyhow!("Error joining thread - {err:?}"))?;
        }
        Ok(())
    }

    pub fn evt_stream(&self) -> &Receiver<SynchronizationEvent> {
        &self.events_stream
    }
}

fn find_mount_info(coord: &SourceCoordinates) -> anyhow::Result<MountedPartitionInfo> {
    match coord {
        SourceCoordinates::Id(id) => crate::common::fs::partition_by_id(&id),
        SourceCoordinates::Path(path) => crate::common::fs::common::partition_by_path(&path),
    }
}

pub fn synchronize_source(opts: SyncOpts, target: &Path) -> anyhow::Result<SyncrhonizationTask> {
    let repo = SourcesRepo::new(target.to_path_buf());
    let (source, source_id) = match opts.source {
        SyncSource::New {
            coord: id,
            name,
            group,
            tags,
        } => {
            let mount_info = find_mount_info(&id)?;
            repo.write_entry(SourceJsonRow {
                id: mount_info.info.partition_id.clone(),
                name,
                group,
                tags,
            })?;
            (mount_info.mount_point, mount_info.info.partition_id)
        }
        SyncSource::Existing { coord: id } => {
            let mount_info = find_mount_info(&id)?;
            repo.find_by_id(&mount_info.info.partition_id)?
                .ok_or_else(|| anyhow::anyhow!("Source {} is not currently registered", mount_info.info.partition_id))?;

            (mount_info.mount_point, mount_info.info.partition_id)
        }
    };

    let (image_path_sender, image_path_receiver) = crossbeam::channel::bounded(100);
    let (record_sender, record_receiver) = crossbeam::channel::bounded(100);
    let (events_sender, events_receiver) = crossbeam::channel::unbounded();
    let (logged_events_sender, logged_events_receiver) = crossbeam::channel::unbounded();

    if opts.count_images {
        thread::spawn({
            let owned_source = source.to_path_buf();
            let owned_events_sender = events_sender.clone();
            move || count_images(owned_source, &owned_events_sender)
        });
    }

    let owned_source = source.to_path_buf();
    let owned_target = target.to_path_buf();
    let scanner_hndl = thread::spawn(move || scan_for_images(owned_source, &image_path_sender));
    let logger_hndl = thread::spawn({
        let owned_target = owned_target.clone();
        let source_id = String::from(&source_id);
        move || {
            logger_worker(
                owned_target,
                source_id,
                events_receiver,
                logged_events_sender,
            )
        }
    });
    let writer_hndl = thread::spawn(move || process_record_store(owned_target, record_receiver));
    let workers_hdnl = (0..4)
        .into_iter()
        .map(|idx| {
            let receiver = image_path_receiver.clone();
            let record_sender = record_sender.clone();
            let events_sender = events_sender.clone();
            let owned_target = target.to_path_buf();
            let owned_source = source.to_path_buf();
            let partition_id = String::from(&source_id);
            thread::spawn(move || {
                process_images(
                    WorkerContext {
                        worker_id: idx,
                        partition_id,
                        source_base_dir: owned_source,
                        target_base_dir: owned_target,
                    },
                    events_sender,
                    record_sender,
                    receiver,
                )
            })
        })
        .collect::<Vec<_>>();

    Ok(SyncrhonizationTask {
        events_stream: logged_events_receiver,
        handlers: [scanner_hndl, writer_hndl, logger_hndl]
            .into_iter()
            .chain(workers_hdnl)
            .collect(),
    })
}

fn logger_worker(
    archive_path: PathBuf,
    source_id: String,
    evt_receiver: Receiver<SynchronizationEvent>,
    evt_sender: Sender<SynchronizationEvent>,
) {
    let now = Utc::now();
    let ignored_log_path = archive_path.join(format!(
        "{}_{}_IGN.log",
        now.format("%Y%m%d-%H%M"),
        source_id
    ));
    let errored_log_path = archive_path.join(format!(
        "{}_{}_ERR.log",
        now.format("%Y%m%d-%H%M"),
        source_id
    ));
    let completed_log_path = archive_path.join(format!(
        "{}_{}_CMP.log",
        now.format("%Y%m%d-%H%M"),
        source_id
    ));

    let mut ignored_f =
        BufWriter::new(File::create(ignored_log_path).expect("Error creating skipped log file"));
    let mut errored_f =
        BufWriter::new(File::create(errored_log_path).expect("Error creating skipped log file"));
    let mut completed_f =
        BufWriter::new(File::create(completed_log_path).expect("Error creating skipped log file"));

    while let Ok(evt) = evt_receiver.recv() {
        let out = match &evt {
            SynchronizationEvent::Stored {
                src,
                dst,
                generated,
                partial,
            } => completed_f
                .write(format!("src: {src:?} dst: {dst:?} gen: {generated} par: {partial}\n").as_bytes()),
            SynchronizationEvent::Skipped { src, existing } => {
                ignored_f.write(format!("src: {src:?} cause: file already exists {existing:?}\n").as_bytes())
            }SynchronizationEvent::Ignored { src, cause } => {
                ignored_f.write(format!("src: {src:?} cause: {cause}\n").as_bytes())
            }
            SynchronizationEvent::Errored { src, cause } => {
                errored_f.write(format!("src: {src:?} cause: '{cause}'\n").as_bytes())
            }
            SynchronizationEvent::ScanProgress { .. }
            | SynchronizationEvent::ScanCompleted { .. } => Ok(0),
        };
        if let Err(err) = out {
            eprintln!("Error writing log - {err}");
        }
        send_or_log(&evt_sender, evt);
    }
}

fn scan_for_images(source: PathBuf, sender: &Sender<PathBuf>) {
    scan_for_images_with_callback(source, &mut |entry| {
        sender.send(entry).expect("Error sending path")
    });
}

fn count_images(source: PathBuf, sender: &Sender<SynchronizationEvent>) {
    let mut count = 0;
    let mut last_evt_sent_ts = SystemTime::now();
    let mut callback = |_entry| {
        count += 1;
        if last_evt_sent_ts.add(Duration::from_millis(1000)) < SystemTime::now() {
            let out = sender.send(SynchronizationEvent::ScanProgress { count });
            last_evt_sent_ts = SystemTime::now();
            if let Err(err) = out {
                eprintln!("Error updating img count - {err}");
            }
        }
    };
    scan_for_images_with_callback(source, &mut callback);

    let out = sender.send(SynchronizationEvent::ScanCompleted { count });
    if let Err(err) = out {
        eprintln!("Error updating img count - {err}");
    }
}

fn scan_for_images_with_callback(source: PathBuf, callback: &mut impl FnMut(PathBuf)) {
    for entry_res in fs::read_dir(&source).expect("Error reading dir") {
        match entry_res {
            Ok(entry) => {
                let entry_path = entry.path();

                if entry_path.is_dir() && !entry_path.is_symlink() {
                    scan_for_images_with_callback(entry_path, callback)
                } else if entry_path.is_file() {
                    let ext = entry_path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(ToString::to_string)
                        .unwrap_or_default()
                        .to_lowercase();

                    let supported_format = ["jpg", "jpeg"].contains(&&ext[..]);
                    if supported_format {
                        callback(entry_path);
                    }
                }
            }
            Err(err) => eprintln!("Error reading dir entry - {err}"),
        }
    }
}

pub struct WorkerContext {
    worker_id: u32,
    partition_id: String,
    source_base_dir: PathBuf,
    target_base_dir: PathBuf,
}

fn send_or_log<T>(sender: &Sender<T>, msg: T) {
    let out = sender.send(msg);
    if let Err(err) = out {
        eprintln!("Error sending to channel - {err}");
    }
}

fn process_images(
    ctx: WorkerContext,
    events_sender: Sender<SynchronizationEvent>,
    record_sender: Sender<PhotoArchiveRow>,
    receiver: Receiver<PathBuf>,
) {
    let partition_crc = CASTAGNOLI.checksum(ctx.partition_id.as_bytes());
    let send_evt = |evt: SynchronizationEvent| send_or_log(&events_sender, evt);

    while let Ok(p) = receiver.recv() {
        let (datetime, exif) = match extract_exif(&p)
            .map(|maybe_exif| maybe_exif.map(|exif| (extract_timestamp(&exif), exif)))
        {
            Err(err) => {
                eprintln!("Error extracting exif data - {err}");
                (None, None)
            }
            Ok(None) => (None, None),
            Ok(Some((None, exif))) => (None, Some(exif)),
            Ok(Some((Some(datetime), exif))) => (Some(datetime), Some(exif)),
        };


        let archive_paths = build_paths(
            partition_crc,
            &ctx.target_base_dir,
            &p.strip_prefix(&ctx.source_base_dir).expect("Error extracting base dir"),
            datetime.as_ref(),
        ).expect("Error building paths");

        if !archive_paths.img_path.exists() {
            fs::create_dir_all(&archive_paths.img_path).expect("Error creating dir");
        }

        if archive_paths.link_file_path.exists() {
            send_evt(SynchronizationEvent::Skipped {
                src: p,
                existing: archive_paths.link_file_path,
            });
            continue;
        } else if !archive_paths.link_dir_path.exists() {
            fs::create_dir_all(&archive_paths.link_dir_path).expect("Error creating dir");
        }

        let out = image::open(p.as_path())
            .map_err(anyhow::Error::from)
            .and_then(|img| {
                if img.height() < 300 || img.width() < 300 {
                    return Ok(ImgProcessOutcome::Ignored { cause: format!("Image is too small {}x{}", img.width(), img.height()) })
                }
                let digest = CASTAGNOLI.checksum(img.as_bytes());
                let file_name = build_filename(
                    datetime.as_ref(),
                    std::fs::metadata(&p)?.modified()?,
                    digest,
                )?;
                let file_path = archive_paths.img_path.join(&file_name);
                let generated = if !file_path.exists() {
                    generate_thumb(&img, file_path.as_path())?;
                    true
                } else {
                    false
                };
                if !archive_paths.link_file_path.exists() {
                    std::os::unix::fs::symlink(
                        PathBuf::from("../img").join(file_name),
                        archive_paths.link_file_path,
                    )?;

                    record_sender
                        .send(PhotoArchiveRow {
                            photo_ts: datetime,
                            file_ts: fs::metadata(&p)?.modified()?,
                            source_id: ctx.partition_id.clone(),
                            source_path: p
                                .strip_prefix(&ctx.source_base_dir)
                                .unwrap()
                                .to_path_buf(),
                            exif,
                            size: fs::metadata(&p)
                                .expect("Cannot extract file metadata")
                                .len(),
                            height: img.height(),
                            width: img.width(),
                            digest,
                        })
                        .expect("Error sending photo archive row");
                }
                Ok(ImgProcessOutcome::Completed { generated, partial: datetime.is_none(), dst_path: file_path })
            });

        match out {
            Err(err) => send_evt(SynchronizationEvent::Errored {
                src: p,
                cause: format!("Error processing image - {err}"),
            }),
            Ok(ImgProcessOutcome::Completed { generated, partial, dst_path }) => send_evt(SynchronizationEvent::Stored {
                src: p,
                dst: dst_path,
                generated,
                partial,
            }),
            Ok(ImgProcessOutcome::Ignored { cause }) => send_evt(SynchronizationEvent::Ignored {
                src: p,
                cause
            }),
        }
    }
}

enum ImgProcessOutcome {
    Completed { generated: bool, partial: bool, dst_path: PathBuf },
    Ignored { cause: String },
}

fn extract_exif(image_path: &Path) -> anyhow::Result<Option<Exif>> {
    let file = std::fs::File::open(&image_path)?;
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let exif = exifreader.read_from_container(&mut bufreader).ok();

    Ok(exif)
}

fn extract_timestamp(exif: &Exif) -> Option<NaiveDateTime> {
    let dt = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
        .or_else(|| exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY))
        .map(|datetime| {
            let datetime_str = datetime.value.display_as(Tag::DateTimeOriginal).to_string();
            NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S")
                .with_context(|| format!("source {datetime_str}"))
        });

    match dt {
        None => None,
        Some(Ok(dt)) => Some(dt),
        Some(Err(err)) => {
            eprintln!("Error parsing datetime - {err}");
            None
        }
    }
}

pub const CASTAGNOLI: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

fn generate_thumb(img: &DynamicImage, target: &Path) -> anyhow::Result<()> {
    let (nheight, nwidth) = if img.height() > img.width() {
        (300, img.width() * 300 / img.height())
    } else {
        (img.height() * 300 / img.width(), 300)
    };

    let resized = img.resize(nwidth, nheight, FilterType::Nearest);
    resized.save_with_format(target, ImageFormat::Jpeg)?;
    Ok(())
}

fn process_record_store(target_base_dir: PathBuf, receiver: Receiver<PhotoArchiveRow>) {
    let store = PhotoArchiveRecordsStore::new(target_base_dir.as_path());
    while let Ok(row) = receiver.recv() {
        store.write(row);
    }
}
