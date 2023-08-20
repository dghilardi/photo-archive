use std::path::{Path, PathBuf};
use std::{fs, thread};
use std::os::unix::prelude::OsStrExt;
use anyhow::Context;
use chrono::{Datelike, NaiveDateTime, NaiveTime, ParseResult};
use crc::{Crc, CRC_32_ISCSI};
use crossbeam::channel::{Receiver, Sender};
use exif::{Exif, Tag};
use image::{DynamicImage, ImageBuffer, ImageFormat};
use image::imageops::FilterType;

pub fn synchronize_source(partition_id: &str, source: &Path, target: &Path) -> anyhow::Result<()> {
    let (image_path_sender, image_path_receiver) = crossbeam::channel::bounded(100);

    let owned_source = source.to_path_buf();
    let scanner_hndl = thread::spawn(move || scan_for_images(owned_source, &image_path_sender));
    let workers_hdnl = (0..4).into_iter()
        .map(|idx| {
            let receiver = image_path_receiver.clone();
            let owned_target = target.to_path_buf();
            let owned_source = source.to_path_buf();
            let partition_id = String::from(partition_id);
            thread::spawn(move || process_images(WorkerContext {
                worker_id: idx,
                partition_id,
                source_base_dir: owned_source,
                target_base_dir: owned_target,
            }, receiver))
        })
        .collect::<Vec<_>>();

    scanner_hndl.join().expect("Scanner join produced error");
    for hndl in workers_hdnl {
        hndl.join().expect("Worker join produced error");
    }
    Ok(())
}

fn scan_for_images(source: PathBuf, sender: &Sender<PathBuf>) {
    for entry_res in fs::read_dir(&source).expect("Error reading dir") {
        match entry_res {
            Ok(entry) => {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    scan_for_images(entry_path, sender)
                } else if entry_path.is_file() {
                    let ext = entry_path.extension()
                        .and_then(|ext| ext.to_str()).map(ToString::to_string)
                        .unwrap_or_default()
                        .to_lowercase();

                    let supported_format = ["jpg", "jpeg"].contains(&&ext[..]);
                    if supported_format {
                        sender
                            .send(entry_path)
                            .expect("Error sending path");
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

fn process_images(ctx: WorkerContext, receiver: Receiver<PathBuf>) {
    let partition_crc = CASTAGNOLI.checksum(ctx.partition_id.as_bytes());
    while let Ok(p) = receiver.recv() {
        match extract_exif(&p) {
            Err(err) => eprintln!("Error processing image - {err}"),
            Ok(maybe_exif) => {
                if let Some(exif) = maybe_exif {
                    if let Some(datetime) = extract_timestamp(&exif) {
                        println!("{datetime:?}");
                        let date_path = ctx.target_base_dir.join(datetime.year().to_string()).join(datetime.format("%m.%d").to_string());
                        let img_path = date_path.join("img");
                        if !img_path.exists() {
                            fs::create_dir_all(&img_path).expect("Error creating dir");
                        }
                        let source_dir = p.parent().expect("No source dir found");
                        let link_path = date_path.join(
                            format!("{:08X}.{:08X}.{}",
                                    partition_crc,
                                    CASTAGNOLI.checksum(source_dir.strip_prefix(&ctx.source_base_dir).expect("Error stripping prefix").as_os_str().as_bytes()),
                                    source_dir.file_name().and_then(|n| n.to_str()).expect("Error extracting parent dir"),
                            )
                        );
                        if !link_path.exists() {
                            fs::create_dir_all(&link_path).expect("Error creating dir");
                        }

                        let out = image::open(p.as_path())
                            .map_err(anyhow::Error::from)
                            .and_then(|img| {
                                let file_path = img_path.join(format!("{}_{:08X}.jpg", datetime.format("%H%M%S"), CASTAGNOLI.checksum(img.as_bytes())));
                                let link_file_path = link_path.join(p.file_name().expect("Error extracting filename"));
                                if !file_path.exists() {
                                    generate_thumb(&img, file_path.as_path())?;
                                }
                                std::os::unix::fs::symlink(&file_path, link_file_path)?;
                                Ok(file_path)
                            });
                        if let Err(err) = out {
                            eprintln!("Error storing thumb {err}");
                        }
                    } else {
                        println!("No datetime for {p:?}");
                    }
                }
            }
        }
    }
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