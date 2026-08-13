use std::{env, process::ExitCode};

use arreat_core::APP_NAME;

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);

    match (args.next(), args.next()) {
        (None, None) => {
            println!("Arreat Index 当前仅提供工程基线，功能仍在建设中。");
            ExitCode::SUCCESS
        }
        (Some(argument), None) if argument == "--version" => {
            println!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("参数无效；当前仅支持 --version。");
            ExitCode::from(2)
        }
    }
}
