#![forbid(unsafe_code)]

use std::{io, path::PathBuf, process::ExitCode};

use arreat_hover_probe::run_capture;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(about = "Read-only diagnostic probe for the D2R hover record")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture the operator-confirmed six-stage hover sequence.
    Capture {
        /// D2R build metadata file used only for a report digest.
        #[arg(long, value_name = "FILE")]
        build_info: PathBuf,
    },
}

fn main() -> ExitCode {
    let Cli { command } = Cli::parse();
    match command {
        Command::Capture { build_info } => ExitCode::from(run_capture(
            &build_info,
            &mut io::stdin().lock(),
            &mut io::stderr().lock(),
            &mut io::stdout().lock(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn help_documents_capture() {
        let mut bytes = Vec::new();
        Cli::command().write_long_help(&mut bytes).unwrap();
        let help = String::from_utf8(bytes).unwrap();
        assert!(help.contains("capture"));
        assert!(help.contains("Read-only diagnostic probe"));
    }
}
