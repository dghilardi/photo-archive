use std::path::PathBuf;
use clap::{Parser, Subcommand};

/// Simple program to index a multi-source photo archive
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct PhotoArchiveArgs {
    #[clap(subcommand)]
    pub subcommand: PhotoArchiveCommand,
}

#[derive(Subcommand, Debug)]
pub enum PhotoArchiveCommand {
    /// List mounted disks that can be used as source
    ListSources,
    /// Import source into archive
    ImportSource {
        /// Id of the source to import
        #[arg(short, long)]
        source_id: String,
        /// Archive path
        #[arg(short, long)]
        target: PathBuf
    },
}