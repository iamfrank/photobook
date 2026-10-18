use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "Generate editable A5 photobooks and print-ready booklet layouts"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a Typst document and snapshot its image assets; does not compile.
    Generate {
        /// Folder of photos (no recursion); generates one photo per page.
        folder: PathBuf,
        /// New .typ file; existing sources/assets are never overwritten.
        #[arg(short, long, default_value = "photobook.typ")]
        output: PathBuf,
    },
    /// Compile a reviewed Typst document to PDF using the embedded Typst compiler.
    Compile {
        source: PathBuf,
        /// Defaults to <source>.pdf, or <source>-booklet.pdf with --booklet.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Impose A5 pages onto A4 landscape sheets for saddle-stitch binding.
        #[arg(long)]
        booklet: bool,
    },
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Generate { folder, output } => {
            let result = photobook::generate(&folder, &output)?;
            for warning in result.warnings {
                eprintln!("Warning: {warning}");
            }
            println!(
                "Created {} ({} images, {} pages).",
                result.source.display(),
                result.images,
                result.pages
            );
            println!("Review/edit the .typ file, then run photobook compile.");
        }
        Commands::Compile {
            source,
            output,
            booklet,
        } => {
            let result = photobook::compile(&source, output.as_deref(), booklet)?;
            for warning in &result.warnings {
                eprintln!("{warning}");
            }
            println!("Created {}", result.output.display());
            if let Some(stats) = result.booklet {
                println!(
                    "{} A5 pages + {} blank pages → {} A4 sheets ({} PDF sides).",
                    stats.pages,
                    stats.blanks,
                    stats.sheets,
                    stats.sheets * 2
                );
                println!("Print A4 landscape, actual size (100%), duplex / flip on short edge.");
                println!("Do not enable the printer's booklet or multiple-pages-per-sheet mode.");
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("photobook: {error:#}");
            ExitCode::FAILURE
        }
    }
}
