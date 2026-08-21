use std::{path::PathBuf, process::ExitCode};

use arreat_data::{Error, export_archive, normalize_to_path, write_audit};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "arreat-data",
    version,
    about = "Read-only D2R data extraction and normalization"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Export {
        #[arg(long)]
        game_root: PathBuf,
        #[arg(long)]
        archive: PathBuf,
    },
    Normalize {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Audit {
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        json: PathBuf,
        #[arg(long)]
        markdown: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("arreat-data: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Export { game_root, archive } => export_archive(&game_root, &archive),
        Command::Normalize { input, output } => {
            normalize_to_path(&input, &output)?;
            Ok(())
        }
        Command::Audit {
            snapshot,
            json,
            markdown,
        } => {
            let report = write_audit(&snapshot, &json, &markdown)?;
            if report.passed {
                Ok(())
            } else {
                Err(Error::Message(format!(
                    "audit failed with {} errors or missing required sentinels",
                    report.error_count
                )))
            }
        }
    }
}
