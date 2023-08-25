use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub struct SourcesRepo {
    archive_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub struct SourceJsonRow {
    pub id: String,
    pub name: String,
    pub group: String,
    pub tags: Vec<String>,
}

impl Display for SourceJsonRow {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\t{}\t[{}]", self.id, self.name, self.group)
    }
}

impl SourcesRepo {
    pub fn new(archive_dir: PathBuf) -> Self {
        Self {
            archive_dir
        }
    }

    fn db_path(&self) -> PathBuf {
        self.archive_dir.join("sources.ndjson")
    }

    pub fn find_by_id(&self, source_id: &str) -> anyhow::Result<Option<SourceJsonRow>> {
        let db_path = self.db_path();
        if db_path.exists() {
            let file = File::open(&db_path)?;
            let reader = BufReader::new(file);

            let entry = reader.lines()
                .map(|res_line| res_line.and_then(|line| Ok(serde_json::from_str::<SourceJsonRow>(&line)?)))
                .filter_map(|entry| entry.ok())
                .find(|entry| entry.id.eq(source_id));

            Ok(entry)
        } else {
            Ok(None)
        }
    }

    pub fn all(&self) -> anyhow::Result<Vec<SourceJsonRow>> {
        let db_path = self.db_path();
        if db_path.exists() {
            let file = File::open(&db_path)?;
            let reader = BufReader::new(file);

            let entry = reader.lines()
                .map(|res_line| res_line.and_then(|line| Ok(serde_json::from_str::<SourceJsonRow>(&line)?)))
                .filter_map(|entry| entry.ok())
                .collect();

            Ok(entry)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn write_entry(&self, entry: SourceJsonRow) -> anyhow::Result<()> {
        if let Some(existing_entry) = self.find_by_id(&entry.id)? {
            anyhow::bail!("Source with id {} is already registered with name '{}'", existing_entry.id, existing_entry.name);
        }
        let new_row = serde_json::to_string(&entry)?;

        let mut db_file = std::fs::File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(self.db_path())?;

        db_file.write(new_row.as_bytes())?;
        db_file.write(b"\n")?;
        Ok(())
    }
}