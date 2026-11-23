//! The command line read by `clap`, with its derive (choice 1796).

use clap::Parser;

/// Packs files into an archive.
#[derive(Parser)]
#[command(name = "pack")]
struct Pack {
    /// Print each file as it is packed
    #[arg(short, long)]
    verbose: bool,
    /// Print nothing but errors
    #[arg(short, long)]
    quiet: bool,
    /// Write over an archive that exists
    #[arg(short, long)]
    force: bool,
    /// Show what would be packed, and pack nothing
    #[arg(short = 'n', long)]
    dry_run: bool,
    /// Write the archive to FILE
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,
    /// Compress at level N
    #[arg(short, long, value_name = "N")]
    level: Option<String>,
    /// Leave out the files that match PATTERN; may be given again
    #[arg(short = 'x', long, value_name = "PATTERN")]
    exclude: Vec<String>,
    /// The kind of archive: zip or tar
    format: String,
    /// The folder to pack
    folder: String,
    /// Files to pack besides the folder
    files: Vec<String>,
}

pub fn read() -> super::Read {
    let p = Pack::parse();
    super::Read {
        verbose: p.verbose,
        quiet: p.quiet,
        force: p.force,
        dry_run: p.dry_run,
        output: p.output,
        level: p.level,
        exclude: p.exclude,
        format: p.format,
        folder: p.folder,
        files: p.files,
    }
}
