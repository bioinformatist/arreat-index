use std::{
    env,
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    str::FromStr,
};

use arreat_core::APP_NAME;
use arreat_data::CanonicalItemId;
use arreat_market::{CurrentAskSummary, Dd373CurrentAskLookup, MarketError};

const MARKET_HELP: &str = "查询 DD373 当前卖家挂单（实验性）\n\n用法:\n  arreat-app market lookup --catalog <PATH> --item <CANONICAL-ID>\n\n选项:\n  --catalog <PATH>          临时生成的名称目录 JSON\n  --item <CANONICAL-ID>     base:r01..base:r33，或目录中的暗金/套装物品\n  -h, --help                显示帮助\n";

enum Command {
    Baseline,
    Version,
    Help,
    Lookup {
        catalog: PathBuf,
        item: CanonicalItemId,
    },
}

fn main() -> ExitCode {
    match parse_args(env::args_os().skip(1).collect()) {
        Ok(command) => run(command),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: Vec<OsString>) -> Result<Command, &'static str> {
    if args.is_empty() {
        return Ok(Command::Baseline);
    }
    if args.len() == 1 && args[0] == "--version" {
        return Ok(Command::Version);
    }
    if args.len() == 3
        && args[0] == "market"
        && args[1] == "lookup"
        && (args[2] == "--help" || args[2] == "-h")
    {
        return Ok(Command::Help);
    }
    if args.len() != 6 || args[0] != "market" || args[1] != "lookup" {
        return Err("参数无效；请运行 arreat-app market lookup --help。");
    }
    let mut catalog = None;
    let mut item = None;
    for pair in args[2..].chunks_exact(2) {
        if pair[0] == "--catalog" && catalog.is_none() {
            catalog = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--item" && item.is_none() {
            item = Some(
                CanonicalItemId::from_str(pair[1].to_str().ok_or("物品 ID 必须是 UTF-8 文本。")?)
                    .map_err(|_| "物品 ID 格式无效。")?,
            );
        } else {
            return Err("参数无效；请运行 arreat-app market lookup --help。");
        }
    }
    Ok(Command::Lookup {
        catalog: catalog.ok_or("缺少 --catalog。")?,
        item: item.ok_or("缺少 --item。")?,
    })
}

fn run(command: Command) -> ExitCode {
    match command {
        Command::Baseline => {
            println!("Arreat Index 当前仅提供工程基线，功能仍在建设中。");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Help => {
            print!("{MARKET_HELP}");
            ExitCode::SUCCESS
        }
        Command::Lookup { catalog, item } => {
            let result = Dd373CurrentAskLookup::from_catalog_path(catalog)
                .and_then(|market| market.lookup(&item));
            write_lookup_result(result, &mut io::stdout().lock(), &mut io::stderr().lock())
        }
    }
}

fn write_lookup_result(
    result: Result<CurrentAskSummary, MarketError>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    match result {
        Ok(summary) => match serde_json::to_writer(&mut *stdout, &summary) {
            Ok(()) if writeln!(stdout).is_ok() => ExitCode::SUCCESS,
            _ => {
                let _ = writeln!(stderr, "结果序列化失败。");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            ExitCode::from(if error.is_invalid_input() { 2 } else { 1 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn old_commands_and_help_are_preserved() {
        assert!(matches!(parse_args(vec![]), Ok(Command::Baseline)));
        assert!(matches!(
            parse_args(args(&["--version"])),
            Ok(Command::Version)
        ));
        assert!(matches!(
            parse_args(args(&["market", "lookup", "--help"])),
            Ok(Command::Help)
        ));
    }
    #[test]
    fn lookup_accepts_both_orders_but_no_extra_flags() {
        assert!(matches!(
            parse_args(args(&[
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17"
            ])),
            Ok(Command::Lookup { .. })
        ));
        assert!(matches!(
            parse_args(args(&[
                "market",
                "lookup",
                "--item",
                "base:r17",
                "--catalog",
                "c"
            ])),
            Ok(Command::Lookup { .. })
        ));
        assert!(
            parse_args(args(&[
                "market",
                "lookup",
                "--catalog",
                "c",
                "--retry",
                "2"
            ]))
            .is_err()
        );
    }

    #[test]
    fn lookup_errors_share_bounded_stderr_and_never_write_json() {
        let cases = [
            (MarketError::InvalidCatalog, 2),
            (MarketError::InvalidInput("测试输入"), 2),
            (MarketError::Network, 1),
            (MarketError::Timeout, 1),
        ];
        for (error, expected) in cases {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let exit = write_lookup_result(Err(error), &mut stdout, &mut stderr);
            assert_eq!(exit, ExitCode::from(expected));
            assert!(stdout.is_empty());
            assert!(!stderr.is_empty());
            assert!(stderr.len() <= 128);
            assert!(!stderr.contains(&b'{'));
            assert_eq!(stderr.last(), Some(&b'\n'));
        }
    }
}
