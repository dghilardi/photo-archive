use std::path::{Path, PathBuf};
use std::{fs, thread};
use anyhow::Context;
use chrono::{NaiveDateTime, NaiveTime, ParseResult};
use crossbeam::channel::{Receiver, Sender};
use exif::{Exif, Tag};

pub fn synchronize_source(source: &Path, target: &Path) -> anyhow::Result<()> {
    let (s, r) = crossbeam::channel::bounded(100);
    let owned_source = source.to_path_buf();
    let scanner_hndl = thread::spawn(move || scan_for_images(owned_source, &s));
    let workers_hdnl = (0..4).into_iter()
        .map(|idx| {
            let receiver = r.clone();
            let owned_target = target.to_path_buf();
            thread::spawn(move || process_images(idx, receiver, owned_target))
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
            },
            Err(err) => eprintln!("Error reading dir entry - {err}"),
        }
    }
}

fn process_images(worker_idx: i32, receiver: Receiver<PathBuf>, target_dir: PathBuf) {
    while let Ok(p) = receiver.recv() {
        match process_image(&p) {
            Err(err) => eprintln!("Error processing image - {err}"),
            Ok(maybe_exif) => {
                if let Some(exif) = maybe_exif {
                    if let Some(datetime) = extract_timestamp(&exif) {
                        println!("{datetime:?}");
                    } else {
                        println!("No datetime for {p:?}");
                    }
                }
            }
        }
    }
}

fn process_image(image_path: &Path) -> anyhow::Result<Option<Exif>> {
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