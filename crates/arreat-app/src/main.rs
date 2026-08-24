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
use arreat_market::{
    CurrentAskSummary, Dd373CurrentAskLookup, MarketError, MarketScope, PlayMode, SeasonScope,
};

const MARKET_HELP: &str = "查询 DD373 当前卖家挂单（实验性）\n\n用法:\n  arreat-app market lookup --catalog <PATH> --item <CANONICAL-ID> [--season <SEASON>] [--mode <MODE>]\n\n选项:\n  --catalog <PATH>          临时生成的名称目录 JSON\n  --item <CANONICAL-ID>     base:r01..base:r33，或目录中的暗金/套装物品\n  --season <SEASON>         non-season 或 latest（默认 non-season）\n  --mode <MODE>             normal 或 hardcore（默认 normal）\n  -h, --help                显示帮助\n";

enum Command {
    Baseline,
    Version,
    Help,
    Lookup {
        catalog: PathBuf,
        item: CanonicalItemId,
        market_scope: MarketScope,
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
    if args.len() < 6
        || !(args.len() - 2).is_multiple_of(2)
        || args[0] != "market"
        || args[1] != "lookup"
    {
        return Err("参数无效；请运行 arreat-app market lookup --help。");
    }
    let mut catalog = None;
    let mut item = None;
    let mut season = None;
    let mut mode = None;
    for pair in args[2..].chunks_exact(2) {
        let value = pair[1].to_str().ok_or("参数值必须是 UTF-8 文本。")?;
        if pair[0] == "--catalog" && catalog.is_none() {
            catalog = Some(PathBuf::from(value));
        } else if pair[0] == "--item" && item.is_none() {
            item = Some(CanonicalItemId::from_str(value).map_err(|_| "物品 ID 格式无效。")?);
        } else if pair[0] == "--season" && season.is_none() {
            season = Some(match value {
                "non-season" => SeasonScope::NonSeason,
                "latest" => SeasonScope::Latest,
                _ => return Err("--season 必须是 non-season 或 latest。"),
            });
        } else if pair[0] == "--mode" && mode.is_none() {
            mode = Some(match value {
                "normal" => PlayMode::Normal,
                "hardcore" => PlayMode::Hardcore,
                _ => return Err("--mode 必须是 normal 或 hardcore。"),
            });
        } else {
            return Err("参数无效；请运行 arreat-app market lookup --help。");
        }
    }
    Ok(Command::Lookup {
        catalog: catalog.ok_or("缺少 --catalog。")?,
        item: item.ok_or("缺少 --item。")?,
        market_scope: MarketScope {
            season: season.unwrap_or(SeasonScope::NonSeason),
            mode: mode.unwrap_or(PlayMode::Normal),
        },
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
        Command::Lookup {
            catalog,
            item,
            market_scope,
        } => {
            let result = Dd373CurrentAskLookup::from_catalog_path(catalog)
                .and_then(|market| market.lookup(&item, market_scope));
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
    use arreat_market::{
        AskStatistics, CurrentAskStatus, ExclusionCounts, PriceType, Pricing, Provider,
    };

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn parsed_scope(values: &[&str]) -> MarketScope {
        match parse_args(args(values)).unwrap() {
            Command::Lookup { market_scope, .. } => market_scope,
            _ => panic!("expected lookup"),
        }
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
        assert_eq!(
            parsed_scope(&["market", "lookup", "--catalog", "c", "--item", "base:r17"]),
            MarketScope::default()
        );
        assert_eq!(
            parsed_scope(&["market", "lookup", "--item", "base:r17", "--catalog", "c"]),
            MarketScope::default()
        );
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
    fn lookup_accepts_all_scope_choices_in_any_pair_order() {
        let base = ["market", "lookup", "--catalog", "c", "--item", "base:r17"];
        for (season_value, season) in [
            ("non-season", SeasonScope::NonSeason),
            ("latest", SeasonScope::Latest),
        ] {
            for (mode_value, mode) in [
                ("normal", PlayMode::Normal),
                ("hardcore", PlayMode::Hardcore),
            ] {
                let mut values = base.to_vec();
                values.extend(["--season", season_value, "--mode", mode_value]);
                assert_eq!(parsed_scope(&values), MarketScope { season, mode });

                let reordered = [
                    "market",
                    "lookup",
                    "--mode",
                    mode_value,
                    "--item",
                    "base:r17",
                    "--season",
                    season_value,
                    "--catalog",
                    "c",
                ];
                assert_eq!(parsed_scope(&reordered), MarketScope { season, mode });
            }
        }
    }

    #[test]
    fn lookup_rejects_invalid_duplicate_missing_and_non_utf8_values() {
        for values in [
            vec![
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--season",
                "old",
            ],
            vec![
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--mode",
                "softcore",
            ],
            vec![
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--season",
                "latest",
                "--season",
                "latest",
            ],
            vec![
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--mode",
                "normal",
                "--mode",
                "hardcore",
            ],
            vec!["market", "lookup", "--catalog", "c", "--item"],
            vec![
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--season",
            ],
        ] {
            assert!(parse_args(args(&values)).is_err(), "accepted {values:?}");
        }

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut values = args(&[
                "market",
                "lookup",
                "--catalog",
                "c",
                "--item",
                "base:r17",
                "--mode",
            ]);
            values.push(OsString::from_vec(vec![0xff]));
            assert!(parse_args(values).is_err());
        }
    }

    #[test]
    fn help_documents_scope_values_and_defaults() {
        for text in [
            "--season <SEASON>",
            "non-season",
            "latest",
            "--mode <MODE>",
            "normal",
            "hardcore",
            "默认 non-season",
            "默认 normal",
        ] {
            assert!(MARKET_HELP.contains(text));
        }
    }

    #[test]
    fn writer_emits_schema_three_scope_and_unavailable_status() {
        let summary = CurrentAskSummary {
            schema_version: 3,
            item_id: CanonicalItemId::from_str("base:r17").unwrap(),
            market_scope: MarketScope {
                season: SeasonScope::Latest,
                mode: PlayMode::Hardcore,
            },
            status: CurrentAskStatus::MarketScopeUnavailable,
            price_type: PriceType::CurrentAsks,
            provider: Provider::Dd373,
            currency: "CNY",
            pricing: Pricing::PerItem {
                unit_price: AskStatistics {
                    minimum: None,
                    median: None,
                },
                entry_price: AskStatistics {
                    minimum: None,
                    median: None,
                },
                offers_at_minimum_unit_price: vec![],
                offers_at_minimum_entry_price: vec![],
            },
            sample_count: 0,
            listing_count: 0,
            exclusions: ExclusionCounts::default(),
            request_count: 4,
            observed_at: "2023-11-14T22:13:20Z".to_owned(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = write_lookup_result(Ok(summary), &mut stdout, &mut stderr);
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        let output: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            output,
            serde_json::json!({
              "schema_version": 3,
              "item_id": "base:r17",
              "market_scope": {
                "season": "latest",
                "mode": "hardcore"
              },
              "status": "market_scope_unavailable",
              "price_type": "current_asks",
              "provider": "dd373",
              "currency": "CNY",
              "pricing": {
                "ask_basis": "per_item",
                "unit_price": {
                  "minimum": null,
                  "median": null
                },
                "entry_price": {
                  "minimum": null,
                  "median": null
                },
                "offers_at_minimum_unit_price": [],
                "offers_at_minimum_entry_price": []
              },
              "sample_count": 0,
              "listing_count": 0,
              "exclusions": {
                "privacy": 0,
                "multi_item": 0,
                "unmatched_item": 0,
                "duplicate_listing": 0,
                "invalid_offer": 0
              },
              "request_count": 4,
              "observed_at": "2023-11-14T22:13:20Z"
            })
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
