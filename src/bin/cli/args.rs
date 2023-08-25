use std::path::PathBuf;
use clap::{Args, Parser, Subcommand};

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
    ImportSource(ImportSourceCliArgs),
    /// Import source into archive
    SyncSource(SyncSourceCliArgs),
    /// Remove source from archive
    RemoveSource(RemoveSourceCliArgs),
}

#[derive(Args, Debug)]
pub struct ImportSourceCliArgs {
    /// Id of the source to import
    #[arg(short, long)]
    pub source_id: Option<String>,
    /// Name of the source to import
    #[arg(long)]
    pub source_name: Option<String>,
    /// Group of the source to import
    #[arg(long)]
    pub source_group: Option<String>,
    /// Group of the source to import
    #[arg(long)]
    pub source_tags: Vec<String>,
    /// Archive path
    #[arg(short, long)]
    pub target: PathBuf,
}

#[derive(Args, Debug)]
pub struct SyncSourceCliArgs {
    /// Id of the source to import
    #[arg(short, long)]
    pub source_id: Option<String>,
    /// Archive path
    #[arg(short, long)]
    pub target: PathBuf,
}

#[derive(Args, Debug)]
pub struct RemoveSourceCliArgs {
    /// Id of the source to remove
    #[arg(short, long)]
    pub source_id: Option<String>,
    /// Archive path
    #[arg(short, long)]
    pub target: PathBuf,
}