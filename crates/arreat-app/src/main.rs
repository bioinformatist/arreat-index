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
use arreat_static_mod::{
    BuildManifest, Error as StaticModError, StaticModConfig, apply_local, d2r_is_running,
    read_applied,
};

const MARKET_HELP: &str = "查询 DD373 当前卖家挂单（实验性）\n\n用法:\n  arreat-app market lookup --catalog <PATH> --item <CANONICAL-ID> [--season <SEASON>] [--mode <MODE>]\n\n选项:\n  --catalog <PATH>          临时生成的名称目录 JSON\n  --item <CANONICAL-ID>     base:r01..base:r33，或目录中的暗金/套装物品\n  --season <SEASON>         non-season 或 latest（默认 non-season）\n  --mode <MODE>             normal 或 hardcore（默认 normal）\n  -h, --help                显示帮助\n";
const STATIC_MOD_HELP: &str = "构建和查看 Arreat Index 本地静态模组（Linux）\n\n用法:\n  arreat-app static-mod apply --game-root <PATH> --explosive-barrels <on|off>\n  arreat-app static-mod status --game-root <PATH>\n\n选项:\n  --game-root <PATH>              D2R 安装根目录\n  --explosive-barrels <on|off>    仅为爆炸桶启用或关闭标记\n  -h, --help                      显示帮助\n";

enum Command {
    Baseline,
    Version,
    Help,
    StaticModHelp,
    Lookup {
        catalog: PathBuf,
        item: CanonicalItemId,
        market_scope: MarketScope,
    },
    StaticModApply {
        game_root: PathBuf,
        config: StaticModConfig,
    },
    StaticModStatus {
        game_root: PathBuf,
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
    if (args.len() == 2 && args[0] == "static-mod" && (args[1] == "--help" || args[1] == "-h"))
        || (args.len() == 3
            && args[0] == "static-mod"
            && (args[1] == "apply" || args[1] == "status")
            && (args[2] == "--help" || args[2] == "-h"))
    {
        return Ok(Command::StaticModHelp);
    }
    if args.len() >= 2 && args[0] == "static-mod" {
        return parse_static_mod(&args);
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

fn parse_static_mod(args: &[OsString]) -> Result<Command, &'static str> {
    if args.len() == 4 && args[1] == "status" && args[2] == "--game-root" {
        return Ok(Command::StaticModStatus {
            game_root: path_value(&args[3])?,
        });
    }
    if args.len() != 6 || args[1] != "apply" {
        return Err("参数无效；请运行 arreat-app static-mod --help。");
    }
    let mut game_root = None;
    let mut explosive_barrels = None;
    for pair in args[2..].chunks_exact(2) {
        if pair[0] == "--game-root" && game_root.is_none() {
            game_root = Some(path_value(&pair[1])?);
        } else if pair[0] == "--explosive-barrels" && explosive_barrels.is_none() {
            explosive_barrels =
                Some(match pair[1].to_str().ok_or("参数值必须是 UTF-8 文本。")? {
                    "on" => true,
                    "off" => false,
                    _ => return Err("--explosive-barrels 必须是 on 或 off。"),
                });
        } else {
            return Err("参数无效；请运行 arreat-app static-mod --help。");
        }
    }
    Ok(Command::StaticModApply {
        game_root: game_root.ok_or("缺少 --game-root。")?,
        config: StaticModConfig {
            explosive_barrels: explosive_barrels.ok_or("缺少 --explosive-barrels。")?,
        },
    })
}

fn path_value(value: &OsString) -> Result<PathBuf, &'static str> {
    value
        .to_str()
        .map(PathBuf::from)
        .ok_or("路径必须是 UTF-8 文本。")
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
        Command::StaticModHelp => {
            print!("{STATIC_MOD_HELP}");
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
        Command::StaticModApply { game_root, config } => write_apply_result(
            apply_local(&game_root, config),
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        ),
        Command::StaticModStatus { game_root } => write_status_result(
            d2r_is_running()
                .and_then(|running| read_applied(&game_root).map(|build| (running, build))),
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        ),
    }
}

fn write_apply_result(
    result: Result<BuildManifest, StaticModError>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    match result {
        Ok(build) => write_static_json(
            serde_json::json!({
                "status": "applied",
                "game_running": false,
                "build": build,
            }),
            stdout,
            stderr,
        ),
        Err(error) => write_static_error(error, stderr),
    }
}

fn write_status_result(
    result: Result<(bool, Option<BuildManifest>), StaticModError>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    match result {
        Ok((game_running, Some(build))) => write_static_json(
            serde_json::json!({
                "status": "applied",
                "game_running": game_running,
                "build": build,
            }),
            stdout,
            stderr,
        ),
        Ok((game_running, None)) => write_static_json(
            serde_json::json!({
                "status": "not_installed",
                "game_running": game_running,
            }),
            stdout,
            stderr,
        ),
        Err(error) => write_static_error(error, stderr),
    }
}

fn write_static_json(
    value: serde_json::Value,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    match serde_json::to_writer(&mut *stdout, &value) {
        Ok(()) if writeln!(stdout).is_ok() => ExitCode::SUCCESS,
        _ => {
            let _ = writeln!(stderr, "结果序列化失败。");
            ExitCode::FAILURE
        }
    }
}

fn write_static_error(error: StaticModError, stderr: &mut impl Write) -> ExitCode {
    let _ = writeln!(stderr, "{error}");
    ExitCode::FAILURE
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
        assert!(matches!(
            parse_args(args(&["static-mod", "--help"])),
            Ok(Command::StaticModHelp)
        ));
    }

    #[test]
    fn static_mod_commands_accept_exact_flags_in_either_apply_order() {
        for (value, expected) in [("on", true), ("off", false)] {
            for values in [
                vec![
                    "static-mod",
                    "apply",
                    "--game-root",
                    "/game",
                    "--explosive-barrels",
                    value,
                ],
                vec![
                    "static-mod",
                    "apply",
                    "--explosive-barrels",
                    value,
                    "--game-root",
                    "/game",
                ],
            ] {
                match parse_args(args(&values)).unwrap() {
                    Command::StaticModApply { game_root, config } => {
                        assert_eq!(game_root, PathBuf::from("/game"));
                        assert_eq!(config.explosive_barrels, expected);
                    }
                    _ => panic!("expected static apply"),
                }
            }
        }
        assert!(matches!(
            parse_args(args(&["static-mod", "status", "--game-root", "/game"])),
            Ok(Command::StaticModStatus { .. })
        ));
    }

    #[test]
    fn static_mod_rejects_missing_duplicate_invalid_and_extra_flags() {
        for values in [
            vec!["static-mod", "apply"],
            vec!["static-mod", "apply", "--game-root", "/game"],
            vec![
                "static-mod",
                "apply",
                "--game-root",
                "/game",
                "--game-root",
                "/other",
            ],
            vec![
                "static-mod",
                "apply",
                "--explosive-barrels",
                "on",
                "--explosive-barrels",
                "off",
            ],
            vec![
                "static-mod",
                "apply",
                "--game-root",
                "/game",
                "--explosive-barrels",
                "yes",
            ],
            vec!["static-mod", "status"],
            vec![
                "static-mod",
                "status",
                "--game-root",
                "/game",
                "--extra",
                "x",
            ],
        ] {
            assert!(parse_args(args(&values)).is_err(), "accepted {values:?}");
        }
    }

    fn manifest() -> BuildManifest {
        BuildManifest {
            schema_version: 1,
            source_build_info_sha256: "a".repeat(64),
            config: StaticModConfig {
                explosive_barrels: true,
            },
            generated_paths: vec![
                "arreat-index-build.json".to_owned(),
                "data/hd/objects/destructibles/barrel_exploding.json".to_owned(),
                "modinfo.json".to_owned(),
            ],
        }
    }

    #[test]
    fn static_mod_writers_emit_exact_json_shapes_and_exit_classes() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            write_apply_result(Ok(manifest()), &mut stdout, &mut stderr),
            ExitCode::SUCCESS
        );
        let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(value["status"], "applied");
        assert_eq!(value["game_running"], false);
        assert_eq!(value["build"]["schema_version"], 1);
        assert!(stderr.is_empty());

        stdout.clear();
        assert_eq!(
            write_status_result(Ok((true, None)), &mut stdout, &mut stderr),
            ExitCode::SUCCESS
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&stdout).unwrap(),
            serde_json::json!({"status":"not_installed","game_running":true})
        );
        stdout.clear();
        assert_eq!(
            write_status_result(Ok((false, Some(manifest()))), &mut stdout, &mut stderr),
            ExitCode::SUCCESS
        );
        assert!(
            serde_json::from_slice::<serde_json::Value>(&stdout)
                .unwrap()
                .get("build")
                .is_some()
        );

        stdout.clear();
        stderr.clear();
        assert_eq!(
            write_apply_result(Err(StaticModError::GameRunning), &mut stdout, &mut stderr),
            ExitCode::FAILURE
        );
        assert!(stdout.is_empty());
        assert!(String::from_utf8(stderr).unwrap().contains("完全退出游戏"));
    }

    #[test]
    fn static_mod_help_is_truthful_and_bounded() {
        for text in [
            "static-mod apply",
            "static-mod status",
            "--game-root <PATH>",
            "--explosive-barrels <on|off>",
            "仅为爆炸桶",
            "Linux",
        ] {
            assert!(STATIC_MOD_HELP.contains(text));
        }
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
