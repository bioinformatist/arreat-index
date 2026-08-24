use std::{
    collections::VecDeque,
    io::{Cursor, Read as _, Write as _},
    net::TcpListener,
    path::PathBuf,
    time::Instant,
};

use serde_json::{Value, json};

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/market")
        .join(name)
}

fn catalog() -> Catalog {
    Catalog::read(&fixture("catalog.json")).unwrap()
}
fn item(value: &str) -> CanonicalItemId {
    value.parse().unwrap()
}

fn scope(season: SeasonScope, mode: PlayMode) -> MarketScope {
    MarketScope { season, mode }
}

struct FakeTransport {
    responses: VecDeque<Result<RawResponse, MarketError>>,
    urls: Vec<String>,
}

impl FakeTransport {
    fn json(values: Vec<Value>) -> Self {
        Self {
            responses: values
                .into_iter()
                .map(|value| {
                    Ok(RawResponse {
                        status: 200,
                        content_type: "application/json; charset=utf-8".to_owned(),
                        body: serde_json::to_vec(&value).unwrap(),
                    })
                })
                .collect(),
            urls: Vec::new(),
        }
    }
}

impl Transport for FakeTransport {
    fn get(&mut self, url: &str) -> Result<RawResponse, MarketError> {
        self.urls.push(url.to_owned());
        self.responses
            .pop_front()
            .expect("one fake response per request")
    }
}

struct FakeClock {
    now: u64,
    sleeps: Vec<Duration>,
}
impl Clock for FakeClock {
    fn now_millis(&mut self) -> u64 {
        self.now
    }
    fn sleep(&mut self, duration: Duration) {
        self.sleeps.push(duration);
        self.now += duration.as_millis() as u64;
    }
}

fn listing_page(records: Value) -> Value {
    json!({"StatusCode":0,"StatusData":{"ResultCode":0,"ResultData":records}})
}

fn named_flow(family: Family) -> Vec<Value> {
    let root_name = match family {
        Family::Unique => "暗金装备&饰品",
        Family::Set => "套装",
        Family::Rune => unreachable!(),
    };
    let children: Vec<_> = (1..=9)
        .map(|number| {
            let name = if matches!(family, Family::Set) && number == 9 {
                "术士".to_owned()
            } else {
                format!("分类{number}")
            };
            json!({"Name":name,"Id":format!("leaf{number:02}")})
        })
        .collect();
    let title = match family {
        Family::Unique => "Alpha Crown",
        Family::Set => "Jade Band",
        Family::Rune => unreachable!(),
    };
    let mut values = vec![
        json!([{"Name":GAME_LABEL,"Id":"game"}]),
        json!([{"Name":root_name,"Id":"root"}]),
        json!([{"Name":"非赛季","Id":"area"}]),
        json!([
            {"Name":"非赛季(术士君临)","Id":"server"},
            {"Name":"非赛季普通","Id":"legacy"}
        ]),
        Value::Array(children),
        listing_page(json!([{"shopno":"one","title":title,"price":"2.50","singleprice":"bad"}])),
    ];
    values.extend((0..8).map(|_| listing_page(json!([]))));
    values
}

fn supported_rune_flow(area_label: &str, server_label: &str) -> Vec<Value> {
    let fixture_values: Vec<Value> =
        serde_json::from_reader(File::open(fixture("rune-flow.json")).unwrap()).unwrap();
    vec![
        fixture_values[0].clone(),
        fixture_values[1].clone(),
        json!([{"Name":area_label,"Id":"area"}]),
        json!([
            {"Name":server_label,"Id":"supported"},
            {"Name":if area_label == "非赛季" { "非赛季普通" } else { "赛季普通" },"Id":"legacy"}
        ]),
        fixture_values[3].clone(),
        fixture_values[4].clone(),
    ]
}

fn spawn_loopback(
    status: &'static str,
    redirect: bool,
) -> (String, std::thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/first");
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        let location = if redirect {
            format!("Location: http://{address}/second\r\n")
        } else {
            String::new()
        };
        write!(
            stream,
            "HTTP/1.1 {status}\r\n{location}Content-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut count = 1;
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    count += 1;
                    let _ = stream.read(&mut request);
                    write!(
                        stream,
                        "HTTP/1.1 {status}\r\n{location}Content-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::yield_now();
                }
                Err(error) => panic!("loopback accept failed: {error}"),
            }
        }
        count
    });
    (url, handle)
}

#[test]
fn rune_flow_is_exact_private_stable_and_rate_limited() {
    let mut values: Vec<Value> =
        serde_json::from_reader(File::open(fixture("rune-flow.json")).unwrap()).unwrap();
    values.insert(2, json!([{"Name":"非赛季","Id":"area"}]));
    values[3] = json!([
        {"Name":"非赛季(术士君临)","Id":"server"},
        {"Name":"非赛季普通","Id":"legacy"}
    ]);
    let mut transport = FakeTransport::json(values);
    let mut clock = FakeClock {
        now: 1_700_000_000_000,
        sleeps: Vec::new(),
    };
    let summary = lookup_with(
        &catalog(),
        &item("base:r17"),
        Family::Rune,
        MarketScope::default(),
        &mut transport,
        &mut clock,
    )
    .unwrap();
    assert_eq!(summary.status, CurrentAskStatus::Resolved);
    assert_eq!(summary.schema_version, 3);
    assert_eq!(summary.market_scope, MarketScope::default());
    assert_eq!(summary.request_count, 6);
    assert_eq!(summary.sample_count, 4);
    assert_eq!(summary.listing_count, 5);
    let Pricing::PerItem {
        unit_price,
        entry_price,
        offers_at_minimum_unit_price,
        offers_at_minimum_entry_price,
    } = &summary.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(unit_price.minimum.unwrap().to_string(), "1.00");
    assert_eq!(unit_price.median.unwrap().to_string(), "1.00");
    assert_eq!(entry_price.minimum.unwrap().to_string(), "2.00");
    assert_eq!(entry_price.median.unwrap().to_string(), "7.50");
    assert_eq!(
        offers_at_minimum_unit_price,
        &vec![
            RuneOffer {
                quantity_per_lot: 5,
                lot_price: Decimal::from_str_exact("5.00").unwrap(),
                available_lots: 2,
                unit_price: Decimal::from_str_exact("1.00").unwrap(),
            },
            RuneOffer {
                quantity_per_lot: 10,
                lot_price: Decimal::from_str_exact("10.00").unwrap(),
                available_lots: 3,
                unit_price: Decimal::from_str_exact("1.00").unwrap(),
            },
        ]
    );
    assert_eq!(
        offers_at_minimum_entry_price,
        &vec![RuneOffer {
            quantity_per_lot: 1,
            lot_price: Decimal::from_str_exact("2.00").unwrap(),
            available_lots: 4,
            unit_price: Decimal::from_str_exact("2.00").unwrap(),
        }]
    );
    assert_eq!(summary.exclusions.privacy, 1);
    assert_eq!(summary.exclusions.duplicate_listing, 1);
    assert_eq!(summary.exclusions.invalid_offer, 1);
    assert_eq!(clock.sleeps, vec![Duration::from_millis(1100); 5]);
    let bytes = serde_json::to_vec(&summary).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&summary).unwrap());
    let text = String::from_utf8(bytes).unwrap();
    for forbidden in ["title", "url", "seller", "contact", "shopno", "raw"] {
        assert!(!text.contains(forbidden));
    }
    assert!(text.contains("\"price_type\":\"current_asks\""));
    assert!(text.contains("\"market_scope\":{\"season\":\"non_season\",\"mode\":\"normal\"}"));
    assert!(text.contains("\"observed_at\":\"2023-11-14T22:13:25Z\""));
    assert!(text.contains(
        "\"quantity_per_lot\":5,\"lot_price\":\"5.00\",\"available_lots\":2,\"unit_price\":\"1.00\""
    ));
}

#[test]
fn named_flows_query_nine_distinct_leaves_once_in_order() {
    for (family, item_id) in [
        (Family::Unique, "unique:alpha-crown"),
        (Family::Set, "set-item:jade-band"),
    ] {
        let mut transport = FakeTransport::json(named_flow(family));
        let mut clock = FakeClock {
            now: 1_700_000_000_000,
            sleeps: Vec::new(),
        };
        let summary = lookup_with(
            &catalog(),
            &item(item_id),
            family,
            MarketScope::default(),
            &mut transport,
            &mut clock,
        )
        .unwrap();
        assert_eq!(summary.request_count, 14);
        let Pricing::PerListing { listing_price } = summary.pricing else {
            panic!("listing pricing")
        };
        assert_eq!(listing_price.minimum.unwrap().to_string(), "2.50");
        let mut expected = vec![
            "https://game.dd373.com/api/game/list".to_owned(),
            "https://game.dd373.com/Api/GameGoodsType/List?parentId=game".to_owned(),
            "https://game.dd373.com/Api/GameOther/List?parentId=game".to_owned(),
            "https://game.dd373.com/Api/GameOther/List?parentId=area".to_owned(),
            "https://game.dd373.com/Api/GameGoodsType/List?parentId=root".to_owned(),
        ];
        expected.extend((1..=9).map(|number| format!(
            "https://goods.dd373.com/Api/Goods/UserCenter/ApiGetShopList?gameid=game&GameOtherId=area_server&GameShopTypeId=leaf{number:02}"
        )));
        assert_eq!(transport.urls, expected);
        assert!(transport.urls.iter().all(|url| !url.contains("legacy")));
        assert_eq!(clock.sleeps, vec![Duration::from_millis(1100); 13]);
    }
}

#[test]
fn every_supported_scope_and_latest_label_synonym_resolves_exactly() {
    let cases = [
        (
            scope(SeasonScope::NonSeason, PlayMode::Normal),
            "非赛季",
            "非赛季(术士君临)",
        ),
        (
            scope(SeasonScope::NonSeason, PlayMode::Hardcore),
            "非赛季",
            "非赛季专家(术士君临)",
        ),
        (
            scope(SeasonScope::Latest, PlayMode::Normal),
            "新赛季",
            "新赛季(术士君临)",
        ),
        (
            scope(SeasonScope::Latest, PlayMode::Hardcore),
            "新赛季",
            "新赛季专家(术士君临)",
        ),
        (
            scope(SeasonScope::Latest, PlayMode::Normal),
            "赛季",
            "赛季(术士君临)",
        ),
        (
            scope(SeasonScope::Latest, PlayMode::Hardcore),
            "赛季",
            "赛季专家(术士君临)",
        ),
    ];
    for (market_scope, area_label, server_label) in cases {
        let mut transport = FakeTransport::json(supported_rune_flow(area_label, server_label));
        let mut clock = FakeClock {
            now: 1_700_000_000_000,
            sleeps: Vec::new(),
        };
        let summary = lookup_with(
            &catalog(),
            &item("base:r17"),
            Family::Rune,
            market_scope,
            &mut transport,
            &mut clock,
        )
        .unwrap();
        assert_eq!(summary.status, CurrentAskStatus::Resolved);
        assert_eq!(summary.market_scope, market_scope);
        assert_eq!(summary.request_count, 6);
        assert!(
            transport
                .urls
                .iter()
                .any(|url| url.contains("GameOtherId=area_supported"))
        );
        assert!(transport.urls.iter().all(|url| !url.contains("legacy")));
    }
}

#[test]
fn supported_scope_absence_is_a_zeroed_schema_three_summary() {
    let item = item("base:r17");
    for (values, expected_requests) in [
        (
            vec![
                json!([{"Name":GAME_LABEL,"Id":"game"}]),
                json!([{"Name":"符文","Id":"root"}]),
                json!([{"Name":"新赛季","Id":"latest"}]),
            ],
            3,
        ),
        (
            vec![
                json!([{"Name":GAME_LABEL,"Id":"game"}]),
                json!([{"Name":"符文","Id":"root"}]),
                json!([{"Name":"非赛季","Id":"area"}]),
                json!([
                    {"Name":"非赛季普通","Id":"legacy-normal"},
                    {"Name":"非赛季专家","Id":"legacy-hardcore"}
                ]),
            ],
            4,
        ),
    ] {
        let mut transport = FakeTransport::json(values);
        let mut clock = FakeClock {
            now: 1_700_000_000_000,
            sleeps: Vec::new(),
        };
        let summary = lookup_with(
            &catalog(),
            &item,
            Family::Rune,
            MarketScope::default(),
            &mut transport,
            &mut clock,
        )
        .unwrap();
        assert_eq!(summary.schema_version, 3);
        assert_eq!(summary.market_scope, MarketScope::default());
        assert_eq!(summary.status, CurrentAskStatus::MarketScopeUnavailable);
        assert_eq!(summary.request_count, expected_requests);
        assert_eq!(summary.sample_count, 0);
        assert_eq!(summary.listing_count, 0);
        assert_eq!(summary.exclusions, ExclusionCounts::default());
        assert_eq!(summary.pricing, empty_pricing(Family::Rune));
        let serialized = serde_json::to_string(&summary).unwrap();
        assert!(serialized.contains("\"status\":\"market_scope_unavailable\""));
        for forbidden in ["title", "url", "seller", "contact", "shopno", "raw"] {
            assert!(!serialized.contains(forbidden));
        }
        assert!(transport.urls.iter().all(|url| !url.contains("legacy")));
    }
}

#[test]
fn area_and_server_taxonomy_fail_closed_on_missing_ambiguous_or_inconsistent_rows() {
    assert!(matches!(
        area_id(&json!([{"Name":"天梯","Id":"x"}]), SeasonScope::Latest),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        area_id(
            &json!([{"Name":"非赛季","Id":"a"},{"Name":"天梯","Id":"b"}]),
            SeasonScope::NonSeason
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        area_id(
            &json!([{"Name":"新赛季","Id":"a"},{"Name":"天梯","Id":"b"}]),
            SeasonScope::Latest
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        area_id(
            &json!([
                {"Name":"新赛季","Id":"a"},
                {"Name":"赛季","Id":"b"}
            ]),
            SeasonScope::Latest
        ),
        Err(MarketError::Taxonomy)
    ));
    assert_eq!(
        area_id(
            &json!([
                {"Name":"新赛季","Id":"a"},
                {"Name":"赛季","Id":"a"}
            ]),
            SeasonScope::Latest
        )
        .unwrap(),
        Some("a".to_owned())
    );
    assert_eq!(
        area_id(
            &json!([{"Name":"非赛季","Id":"abcdef123456"}]),
            SeasonScope::NonSeason
        )
        .unwrap(),
        Some("abcdef123456".to_owned())
    );
    assert_eq!(
        area_id(
            &json!([{"Name":"赛季","Id":"123e4567-e89b-12d3-a456-426614174000"}]),
            SeasonScope::Latest
        )
        .unwrap(),
        Some("123e4567-e89b-12d3-a456-426614174000".to_owned())
    );
    assert!(matches!(
        area_id(&json!([{"Name":"非赛季","Id":""}]), SeasonScope::NonSeason),
        Err(MarketError::Taxonomy)
    ));
    let default = MarketScope::default();
    assert!(matches!(
        server_id(&json!([{"Name":"普通","Id":"x"}]), default),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([{"Name":"非赛季普通","Id":"x"},{"Name":"天梯","Id":"y"}]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([{"Name":"非赛季(术士君临)","Id":"x"},{"Name":"天梯","Id":"y"}]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","Id":"a"},
                {"Name":"非赛季(术士君临)","Id":"b"}
            ]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert_eq!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","Id":"a"},
                {"Name":"非赛季(术士君临)","Id":"a"}
            ]),
            default
        )
        .unwrap(),
        Some("a".to_owned())
    );
    assert_eq!(
        server_id(
            &json!([{"Name":"非赛季(术士君临)","Id":"123e4567-e89b-12d3-a456-426614174000"}]),
            default
        )
        .unwrap(),
        Some("123e4567-e89b-12d3-a456-426614174000".to_owned())
    );
    assert!(matches!(
        server_id(&json!([{"Name":"非赛季(术士君临)","Id":""}]), default),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(&json!([{"Name":"赛季(术士君临)","Id":"latest"}]), default),
        Err(MarketError::Taxonomy)
    ));

    assert!(matches!(
        area_id(
            &json!([
                {"Name":"非赛季","Id":"area"},
                {"Name":"非赛季"}
            ]),
            SeasonScope::NonSeason
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","Id":"server"},
                {"Name":"非赛季普通"}
            ]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    for bad_id in [json!(123), Value::Null, json!({"k":"v"}), json!([1])] {
        assert!(matches!(
            area_id(
                &json!([
                    {"Name":"非赛季","Id":"legacy"},
                    {"Name":"新赛季","Id":bad_id}
                ]),
                SeasonScope::Latest
            ),
            Err(MarketError::Taxonomy)
        ));
        assert!(matches!(
            server_id(
                &json!([
                    {"Name":"非赛季普通","Id":"legacy"},
                    {"Name":"非赛季(术士君临)","Id":bad_id}
                ]),
                default
            ),
            Err(MarketError::Taxonomy)
        ));
    }
    assert!(matches!(
        area_id(
            &json!([
                {"Name":"非赛季","Id":"x"},
                {"Id":"y"}
            ]),
            SeasonScope::NonSeason
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","Id":"x"},
                {"Id":"y"}
            ]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        area_id(
            &json!([
                {"Name":"非赛季","name":"赛季","Id":"a"}
            ]),
            SeasonScope::NonSeason
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","name":"新赛季(术士君临)","Id":"a"}
            ]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        area_id(
            &json!([
                {"Name":"非赛季","Id":"a","id":"b"}
            ]),
            SeasonScope::NonSeason
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","Id":"a","id":"b"}
            ]),
            default
        ),
        Err(MarketError::Taxonomy)
    ));
    assert_eq!(
        area_id(
            &json!([
                {"Name":"非赛季","name":"非赛季","Id":"dup","id":"dup"},
                {"Name":"新赛季","Id":"legacy"}
            ]),
            SeasonScope::NonSeason
        )
        .unwrap(),
        Some("dup".to_owned())
    );
    assert_eq!(
        server_id(
            &json!([
                {"Name":"非赛季(术士君临)","name":"非赛季(术士君临)","Id":"dup","id":"dup"},
                {"Name":"非赛季普通","Id":"legacy"}
            ]),
            default
        )
        .unwrap(),
        Some("dup".to_owned())
    );
}

#[test]
fn area_and_server_unsafe_ids_are_taxonomy_and_are_not_used_in_requests() {
    const BAD_IDS: [&str; 7] = ["a b", "a&b", "a?b", "a#b", "a/b", "a%b", "中文"];
    for bad in BAD_IDS {
        let mut transport = FakeTransport::json(vec![
            json!([{"Name":GAME_LABEL,"Id":"game"}]),
            json!([{"Name":"符文","Id":"root"}]),
            json!([{"Name":"非赛季","Id":bad}]),
        ]);
        let mut clock = FakeClock {
            now: 1_700_000_000_000,
            sleeps: Vec::new(),
        };
        assert!(matches!(
            lookup_with(
                &catalog(),
                &item("base:r17"),
                Family::Rune,
                MarketScope::default(),
                &mut transport,
                &mut clock
            ),
            Err(MarketError::Taxonomy)
        ));
        assert_eq!(transport.urls.len(), 3);
        assert!(transport.urls.iter().all(|url| !url.contains(bad)));
        assert!(!transport.urls.iter().any(|url| url.contains("legacy")));

        let mut transport = FakeTransport::json(vec![
            json!([{"Name":GAME_LABEL,"Id":"game"}]),
            json!([{"Name":"符文","Id":"root"}]),
            json!([{"Name":"非赛季","Id":"area"}]),
            json!([{"Name":"非赛季(术士君临)","Id":bad}, {"Name":"非赛季普通","Id":"legacy"}]),
        ]);
        let mut clock = FakeClock {
            now: 1_700_000_000_000,
            sleeps: Vec::new(),
        };
        assert!(matches!(
            lookup_with(
                &catalog(),
                &item("base:r17"),
                Family::Rune,
                MarketScope::default(),
                &mut transport,
                &mut clock
            ),
            Err(MarketError::Taxonomy)
        ));
        assert_eq!(transport.urls.len(), 4);
        assert!(transport.urls.iter().all(|url| !url.contains(bad)));
        assert!(
            transport
                .urls
                .iter()
                .all(|url| !url.contains("GameOtherId"))
        );
    }
}

#[test]
fn admission_rejects_unsupported_before_transport() {
    let market = Dd373CurrentAskLookup {
        catalog: catalog(),
        client: Client::new(),
    };
    for unsupported in [
        "base:cap",
        "base:r00",
        "base:r34",
        "base:r1",
        "runeword:enigma",
        "unique:missing",
        "set-item:missing",
    ] {
        assert!(matches!(
            market.admit(&item(unsupported)),
            Err(MarketError::InvalidInput(_))
        ));
    }
    assert!(matches!(market.admit(&item("base:r01")), Ok(Family::Rune)));
}

#[test]
fn unique_and_set_use_listing_price_and_ignore_unit_price_metadata() {
    let catalog = catalog();
    for (family, item_id, candidates, title) in [
        (
            Family::Unique,
            "unique:alpha-crown",
            &catalog.candidate_groups.unique,
            "Alpha Crown",
        ),
        (
            Family::Set,
            "set-item:jade-band",
            &catalog.candidate_groups.set,
            "Jade Band",
        ),
    ] {
        let odd = vec![
            json!({"shopno":"1","title":title,"price":"1"}),
            json!({"shopno":"2","title":title,"price":"3","unit":"","singleprice":"bad"}),
            json!({"shopno":"3","title":title,"price":"5","unit":"元/件"}),
        ];
        let odd = summarize(
            &item(item_id),
            family,
            candidates,
            odd,
            MarketScope::default(),
            13,
            0,
        )
        .unwrap();
        assert_eq!(odd.status, CurrentAskStatus::Resolved);
        assert_eq!(odd.sample_count, 3);
        assert_eq!(odd.listing_count, 3);
        assert_eq!(
            odd.pricing,
            Pricing::PerListing {
                listing_price: AskStatistics {
                    minimum: Some(Decimal::ONE),
                    median: Some(Decimal::from(3)),
                },
            }
        );

        let even_with_invalid_prices = vec![
            json!({"shopno":"1","title":title,"price":"1","unit":""}),
            json!({"shopno":"2","title":title,"price":"3","singleprice":"bad"}),
            json!({"shopno":"3","title":title,"price":"5"}),
            json!({"shopno":"4","title":title,"price":"7","unit":"元/件","singleprice":null}),
            json!({"shopno":"5","title":title}),
            json!({"shopno":"6","title":title,"price":"bad"}),
            json!({"shopno":"7","title":title,"price":0}),
            json!({"shopno":"8","title":title,"price":-1}),
        ];
        let even = summarize(
            &item(item_id),
            family,
            candidates,
            even_with_invalid_prices,
            MarketScope::default(),
            13,
            0,
        )
        .unwrap();
        assert_eq!(even.sample_count, 4);
        assert_eq!(even.listing_count, 8);
        assert_eq!(even.exclusions.invalid_offer, 4);
        assert_eq!(
            even.pricing,
            Pricing::PerListing {
                listing_price: AskStatistics {
                    minimum: Some(Decimal::ONE),
                    median: Some(Decimal::from(4)),
                },
            }
        );
    }
}

#[test]
fn named_matching_uses_all_catalog_layers_and_filters_before_price() {
    let candidates = &catalog().candidate_groups.unique;
    let records = vec![
        json!({"shopno":"1","title":"Premium Alpha Crown","price":"2","singleprice":"bad"}),
        json!({"shopno":"2","title":"阿尔法王冠","price":"4"}),
        json!({"shopno":"3","title":"皇冠别名","price":"8","unit":""}),
        json!({"shopno":"4","title":"Alpha Crown Red Moon","singleprice":"bad","unit":""}),
        json!({"shopno":"5","title":"Unrelated","singleprice":"bad","unit":""}),
        json!({"shopno":"6","title":"微信123456","singleprice":"bad","unit":""}),
    ];
    let result = summarize(
        &item("unique:alpha-crown"),
        Family::Unique,
        candidates,
        records,
        MarketScope::default(),
        13,
        0,
    )
    .unwrap();
    assert_eq!(result.sample_count, 3);
    assert!(
        matches!(result.pricing, Pricing::PerListing { listing_price } if listing_price.median == Some(Decimal::from(4)))
    );
    assert_eq!(result.exclusions.multi_item, 1);
    assert_eq!(result.exclusions.unmatched_item, 1);
    assert_eq!(result.exclusions.privacy, 1);
}

#[test]
fn rune_statistics_cover_odd_and_even_unit_and_entry_medians() {
    let odd_asks = vec![
        json!({"shopno":"1","title":"x","amount":1,"price":"1","number":1,"singleprice":"1"}),
        json!({"shopno":"2","title":"x","amount":2,"price":"4","number":2,"singleprice":"2"}),
        json!({"shopno":"3","title":"x","amount":3,"price":"9","number":3,"singleprice":"3"}),
    ];
    let odd = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        odd_asks.clone(),
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        entry_price,
        ..
    } = odd.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(unit_price.median, Some(Decimal::from(2)));
    assert_eq!(entry_price.median, Some(Decimal::from(4)));

    let mut even_asks = odd_asks;
    even_asks.push(
        json!({"shopno":"4","title":"x","amount":4,"price":"16","number":4,"singleprice":"4"}),
    );
    let even = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        even_asks,
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        entry_price,
        ..
    } = even.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(
        unit_price.median,
        Some(Decimal::from_str_exact("2.5").unwrap())
    );
    assert_eq!(
        entry_price.median,
        Some(Decimal::from_str_exact("6.5").unwrap())
    );
}

#[test]
fn rune_invalid_fields_do_not_fallback_and_ratio_contradiction_fails_closed() {
    let cases = [
        ("missing amount", "amount", None),
        ("malformed amount", "amount", Some(json!("bad"))),
        ("zero amount", "amount", Some(json!(0))),
        ("negative amount", "amount", Some(json!(-1))),
        ("non-integral amount", "amount", Some(json!(1.5))),
        (
            "out-of-u64 amount",
            "amount",
            Some(json!("18446744073709551616")),
        ),
        ("missing price", "price", None),
        ("malformed price", "price", Some(json!("bad"))),
        ("zero price", "price", Some(json!(0))),
        ("negative price", "price", Some(json!(-1))),
        ("missing number", "number", None),
        ("malformed number", "number", Some(json!("bad"))),
        ("zero number", "number", Some(json!(0))),
        ("negative number", "number", Some(json!(-1))),
        ("non-integral number", "number", Some(json!(1.5))),
        (
            "out-of-u64 number",
            "number",
            Some(json!("18446744073709551616")),
        ),
        ("missing singleprice", "singleprice", None),
        ("malformed singleprice", "singleprice", Some(json!("bad"))),
        ("zero singleprice", "singleprice", Some(json!(0))),
        ("negative singleprice", "singleprice", Some(json!(-1))),
    ];
    for (name, field, value) in cases {
        let mut record = json!({
            "shopno":"1",
            "title":"x",
            "amount":1,
            "price":1,
            "number":1,
            "singleprice":1
        });
        let object = record.as_object_mut().unwrap();
        if let Some(value) = value {
            object.insert(field.to_owned(), value);
        } else {
            object.remove(field);
        }
        let result = summarize(
            &item("base:r17"),
            Family::Rune,
            &[],
            vec![record],
            MarketScope::default(),
            5,
            0,
        )
        .unwrap();
        assert_eq!(result.exclusions.invalid_offer, 1, "{name}");
        assert_eq!(result.listing_count, 1, "{name}");
        assert_eq!(result.sample_count, 0, "{name}");
        assert_eq!(
            result.status,
            CurrentAskStatus::NoComparableCurrentAsks,
            "{name}"
        );
        assert_eq!(result.pricing, empty_pricing(Family::Rune), "{name}");
    }

    let all_invalid = json!({"shopno":"1","title":"x"});
    let all_invalid = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        vec![all_invalid],
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    assert_eq!(all_invalid.exclusions.invalid_offer, 1);

    let contradiction = vec![json!({
        "shopno":"1",
        "title":"x",
        "amount":2,
        "price":4,
        "number":1,
        "singleprice":"2.01"
    })];
    assert!(matches!(
        summarize(
            &item("base:r17"),
            Family::Rune,
            &[],
            contradiction,
            MarketScope::default(),
            5,
            0
        ),
        Err(MarketError::OfferRatio)
    ));

    let grouped_low_unit_price = vec![json!({
        "shopno":"2",
        "title":"x",
        "amount":10000,
        "price":"0.01",
        "number":1,
        "singleprice":"0.000002"
    })];
    assert!(matches!(
        summarize(
            &item("base:r17"),
            Family::Rune,
            &[],
            grouped_low_unit_price,
            MarketScope::default(),
            5,
            0
        ),
        Err(MarketError::OfferRatio)
    ));
}

#[test]
fn ratio_tolerance_does_not_define_price_ties() {
    let offers = vec![
        json!({"shopno":"1","title":"x","amount":1,"price":"1.0000005","number":2,"singleprice":"1"}),
        json!({"shopno":"2","title":"x","amount":1,"price":"1","number":3,"singleprice":"1.0000005"}),
    ];
    let result = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        offers,
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        entry_price,
        offers_at_minimum_unit_price,
        offers_at_minimum_entry_price,
    } = result.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(unit_price.minimum, Some(Decimal::ONE));
    assert_eq!(entry_price.minimum, Some(Decimal::ONE));
    assert_eq!(offers_at_minimum_unit_price.len(), 1);
    assert_eq!(
        offers_at_minimum_unit_price[0].lot_price,
        Decimal::from_str_exact("1.0000005").unwrap()
    );
    assert_eq!(offers_at_minimum_entry_price.len(), 1);
    assert_eq!(
        offers_at_minimum_entry_price[0].unit_price,
        Decimal::from_str_exact("1.0000005").unwrap()
    );

    let tiny_offers = vec![
        json!({"shopno":"3","title":"x","amount":10,"price":"5","number":1,"singleprice":"0.5"}),
        json!({"shopno":"4","title":"x","amount":10,"price":"5.000007","number":1,"singleprice":"0.5000007"}),
    ];
    let result = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        tiny_offers,
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        offers_at_minimum_unit_price,
        ..
    } = result.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(
        unit_price.minimum,
        Some(Decimal::from_str_exact("0.5").unwrap())
    );
    assert_eq!(offers_at_minimum_unit_price.len(), 1);
    assert_eq!(
        offers_at_minimum_unit_price[0].unit_price,
        Decimal::from_str_exact("0.5").unwrap()
    );
}

#[test]
fn non_resolved_results_preserve_family_pricing_variants() {
    let catalog = catalog();
    let cases = [
        (Family::Rune, "base:r17", &[][..], "x"),
        (
            Family::Unique,
            "unique:alpha-crown",
            &catalog.candidate_groups.unique[..],
            "Alpha Crown",
        ),
        (
            Family::Set,
            "set-item:jade-band",
            &catalog.candidate_groups.set[..],
            "Jade Band",
        ),
    ];
    for (family, item_id, candidates, title) in cases {
        let empty = summarize(
            &item(item_id),
            family,
            candidates,
            vec![],
            MarketScope::default(),
            5,
            0,
        )
        .unwrap();
        assert_eq!(empty.status, CurrentAskStatus::NoComparableCurrentAsks);
        assert_eq!(empty.listing_count, 0);
        assert_eq!(empty.sample_count, 0);
        assert_eq!(empty.pricing, empty_pricing(family));

        let invalid_record = json!({"shopno":"1","title":title});
        let invalid = summarize(
            &item(item_id),
            family,
            candidates,
            vec![invalid_record],
            MarketScope::default(),
            5,
            0,
        )
        .unwrap();
        assert_eq!(invalid.status, CurrentAskStatus::NoComparableCurrentAsks);
        assert_eq!(invalid.listing_count, 1);
        assert_eq!(invalid.sample_count, 0);
        assert_eq!(invalid.exclusions.invalid_offer, 1);
        assert_eq!(invalid.pricing, empty_pricing(family));

        let unavailable = unavailable_summary(&item(item_id), family, MarketScope::default(), 3, 0);
        assert_eq!(unavailable.status, CurrentAskStatus::MarketScopeUnavailable);
        assert_eq!(unavailable.listing_count, 0);
        assert_eq!(unavailable.sample_count, 0);
        assert_eq!(unavailable.pricing, empty_pricing(family));
    }
}

#[test]
fn even_medians_are_overflow_safe_for_listing_and_item_prices() {
    let catalog = catalog();
    let max = Decimal::MAX.to_string();
    let listing = summarize(
        &item("unique:alpha-crown"),
        Family::Unique,
        &catalog.candidate_groups.unique,
        vec![
            json!({"shopno":"1","title":"Alpha Crown","price":max}),
            json!({"shopno":"2","title":"Alpha Crown","price":max}),
        ],
        MarketScope::default(),
        13,
        0,
    )
    .unwrap();
    let Pricing::PerListing { listing_price } = listing.pricing else {
        panic!("listing pricing")
    };
    assert_eq!(listing_price.median, Some(Decimal::MAX));

    let lower = Decimal::MAX - Decimal::from(2);
    let upper = Decimal::MAX;
    let item_summary = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        vec![
            json!({"shopno":"1","title":"x","amount":1,"price":lower.to_string(),"number":1,"singleprice":lower.to_string()}),
            json!({"shopno":"2","title":"x","amount":1,"price":upper.to_string(),"number":1,"singleprice":upper.to_string()}),
        ],
        MarketScope::default(),
        5,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        entry_price,
        ..
    } = item_summary.pricing
    else {
        panic!("item pricing")
    };
    let expected = Decimal::MAX - Decimal::ONE;
    assert_eq!(unit_price.median, Some(expected));
    assert_eq!(entry_price.median, Some(expected));
}

#[test]
fn taxonomy_requires_exact_unique_shapes() {
    assert!(exact_id(&json!([{"Name":"符文","Id":"a"}]), "符文").is_ok());
    assert!(matches!(
        exact_id(&json!([]), "符文"),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        exact_id(
            &json!([{"Name":"符文","Id":"a"},{"Name":"符文","Id":"b"}]),
            "符文"
        ),
        Err(MarketError::Taxonomy)
    ));
    let flow: Vec<Value> =
        serde_json::from_reader(File::open(fixture("rune-flow.json")).unwrap()).unwrap();
    assert_eq!(
        leaves_for(&flow[3], Family::Rune, &item("base:r17")).unwrap(),
        vec!["r17"]
    );
    let mut missing = flow[3].as_array().unwrap().clone();
    missing.pop();
    assert!(matches!(
        leaves_for(&Value::Array(missing), Family::Rune, &item("base:r17")),
        Err(MarketError::Taxonomy)
    ));

    let unique: Vec<_> = (1..=9)
        .map(|number| json!({"Name":format!("n{number}"),"Id":format!("id{number}")}))
        .collect();
    assert_eq!(
        leaves_for(
            &Value::Array(unique.clone()),
            Family::Unique,
            &item("unique:alpha-crown")
        )
        .unwrap()
        .len(),
        9
    );
    let mut duplicate_id = unique.clone();
    duplicate_id[8]["Id"] = json!("id8");
    assert!(matches!(
        leaves_for(
            &Value::Array(duplicate_id),
            Family::Unique,
            &item("unique:alpha-crown")
        ),
        Err(MarketError::Taxonomy)
    ));
    assert!(matches!(
        leaves_for(
            &Value::Array(unique[..8].to_vec()),
            Family::Unique,
            &item("unique:alpha-crown")
        ),
        Err(MarketError::Taxonomy)
    ));
}

#[test]
fn response_validation_covers_status_content_challenge_shape_and_limit() {
    let cases = [
        (
            RawResponse {
                status: 429,
                content_type: "application/json".into(),
                body: b"{}".to_vec(),
            },
            "http",
        ),
        (
            RawResponse {
                status: 200,
                content_type: "text/html".into(),
                body: b"{}".to_vec(),
            },
            "content",
        ),
        (
            RawResponse {
                status: 200,
                content_type: "application/json".into(),
                body: br#"{"captcha":true}"#.to_vec(),
            },
            "challenge",
        ),
        (
            RawResponse {
                status: 200,
                content_type: "application/json".into(),
                body: b"{".to_vec(),
            },
            "json",
        ),
    ];
    for (raw, expected) in cases {
        let mut transport = FakeTransport {
            responses: VecDeque::from([Ok(raw)]),
            urls: vec![],
        };
        let mut clock = FakeClock {
            now: 0,
            sleeps: vec![],
        };
        let mut session = Session {
            transport: &mut transport,
            clock: &mut clock,
            requests: 0,
            last_start: None,
        };
        let error = session.json("https://game.dd373.com/test").unwrap_err();
        assert!(match expected {
            "http" => matches!(error, MarketError::Http(429)),
            "content" => matches!(error, MarketError::ContentType),
            "challenge" => matches!(error, MarketError::Challenge),
            _ => matches!(error, MarketError::InvalidJson),
        });
    }
    let values = vec![json!({}); MAX_REQUESTS + 1];
    let mut transport = FakeTransport::json(values);
    let mut clock = FakeClock {
        now: 0,
        sleeps: vec![],
    };
    let mut session = Session {
        transport: &mut transport,
        clock: &mut clock,
        requests: 0,
        last_start: None,
    };
    for _ in 0..MAX_REQUESTS {
        session.json("https://game.dd373.com/test").unwrap();
    }
    assert!(matches!(
        session.json("https://game.dd373.com/test"),
        Err(MarketError::RequestLimit)
    ));
    assert_eq!(transport.urls.len(), MAX_REQUESTS);
}

#[test]
fn mime_validation_is_exact_after_parameters_and_trimming() {
    for accepted in [
        "application/json",
        " Application/JSON ",
        "application/json; charset=utf-8",
        " application/json ; charset=utf-8",
    ] {
        assert!(is_json_content_type(accepted));
    }
    for rejected in [
        "application/jsonp",
        "application/json-seq",
        "application/jsonsuffix; charset=utf-8",
        "text/application/json",
        "",
    ] {
        assert!(!is_json_content_type(rejected));
    }
}

#[test]
fn bounded_reader_accepts_exact_limit_and_rejects_one_more_byte() {
    let mut exact = Cursor::new(vec![b'x'; MAX_BODY_BYTES]);
    assert_eq!(read_bounded(&mut exact).unwrap().len(), MAX_BODY_BYTES);
    let mut over = Cursor::new(vec![b'x'; MAX_BODY_BYTES + 1]);
    assert!(matches!(
        read_bounded(&mut over),
        Err(MarketError::BodyTooLarge)
    ));
}

#[test]
fn numeric_response_preserves_high_precision_lexeme() {
    const PRICE: &str = "0.1234567890123456789012345678";
    const LOT_PRICE: &str = "0.2469135780246913578024691356";
    let body = format!(
        r#"{{"StatusCode":0,"StatusData":{{"ResultCode":0,"ResultData":[{{"shopno":"one","title":"x","amount":2,"price":{LOT_PRICE},"number":3,"singleprice":{PRICE}}}]}}}}"#
    );
    let mut transport = FakeTransport {
        responses: VecDeque::from([Ok(RawResponse {
            status: 200,
            content_type: "application/json".to_owned(),
            body: body.into_bytes(),
        })]),
        urls: Vec::new(),
    };
    let mut clock = FakeClock {
        now: 0,
        sleeps: Vec::new(),
    };
    let mut session = Session {
        transport: &mut transport,
        clock: &mut clock,
        requests: 0,
        last_start: None,
    };
    let response = session.json("https://goods.dd373.com/test").unwrap();
    let summary = summarize(
        &item("base:r17"),
        Family::Rune,
        &[],
        listing_records(&response).unwrap(),
        MarketScope::default(),
        1,
        0,
    )
    .unwrap();
    let Pricing::PerItem {
        unit_price,
        entry_price,
        ..
    } = summary.pricing
    else {
        panic!("rune pricing")
    };
    assert_eq!(unit_price.minimum.unwrap().to_string(), PRICE);
    assert_eq!(entry_price.minimum.unwrap().to_string(), LOT_PRICE);
}

#[test]
fn production_client_does_not_follow_redirects_or_retry_failures() {
    let client = build_client().unwrap();
    let (url, server) = spawn_loopback("302 Found", true);
    assert_eq!(client.get(url).send().unwrap().status().as_u16(), 302);
    assert_eq!(server.join().unwrap(), 1);

    let (url, server) = spawn_loopback("503 Service Unavailable", false);
    assert_eq!(client.get(url).send().unwrap().status().as_u16(), 503);
    assert_eq!(server.join().unwrap(), 1);
}

#[test]
fn outbound_boundary_accepts_only_exact_https_dd373_hosts() {
    for accepted in [
        "https://game.dd373.com/api/game/list",
        "https://goods.dd373.com/path?next=http://example.test",
    ] {
        assert!(is_allowed_dd373_url(accepted));
    }
    for rejected in [
        "http://game.dd373.com/api/game/list",
        "https://evil.example/api",
        "https://game.dd373.com.evil.example/api",
        "https://game.dd373.com:444/api",
        "https://user@game.dd373.com/api",
        "not a url",
    ] {
        assert!(!is_allowed_dd373_url(rejected));
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = build_client().unwrap();
    let mut transport = ReqwestTransport { client: &client };
    assert!(matches!(
        transport.get(&format!(
            "http://{}/downgrade",
            listener.local_addr().unwrap()
        )),
        Err(MarketError::Network)
    ));
    assert!(matches!(
        listener.accept(),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
    ));
}

#[test]
fn fixed_transport_controls_and_catalog_validation_are_locked() {
    assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(20));
    assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(60));
    assert_eq!(REQUEST_INTERVAL_MS, 1100);
    assert_eq!(MAX_REQUESTS, 16);
    assert_eq!(MAX_BODY_BYTES, 2 * 1024 * 1024);
    let duplicate: Catalog = serde_json::from_value(json!({
        "catalog_version":1,"canonical_ids":["unique:a","unique:a"],
        "candidate_groups":{"unique":[],"set":[]}
    }))
    .unwrap();
    assert!(matches!(
        duplicate.validate(),
        Err(MarketError::InvalidCatalog)
    ));
}
