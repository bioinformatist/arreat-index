//! Privacy-preserving lookup of DD373 current seller asks.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arreat_data::{
    CanonicalItemId, ItemKind, NameCandidate as Candidate, NameCatalog as Catalog,
    normalize_catalog_name,
};
use regex::{Regex, RegexBuilder};
use reqwest::{Url, blocking::Client, header::CONTENT_TYPE, redirect::Policy};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

const GAME_LABEL: &str = "暗黑2：重制版国服";
const USER_AGENT: &str = "Arreat-Index-Current-Asks/0.1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_INTERVAL_MS: u64 = 1_100;
const MAX_REQUESTS: usize = 16;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const CURRENCY: &str = "CNY";
const RATIO_TOLERANCE: Decimal = Decimal::from_parts(1, 0, 0, false, 6);

/// A successful, aggregate-only observation of current seller asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentAskSummary {
    pub schema_version: u32,
    pub item_id: CanonicalItemId,
    pub market_scope: MarketScope,
    pub status: CurrentAskStatus,
    pub price_type: PriceType,
    pub provider: Provider,
    pub currency: &'static str,
    pub pricing: Pricing,
    pub sample_count: usize,
    pub listing_count: usize,
    pub exclusions: ExclusionCounts,
    pub request_count: usize,
    pub observed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentAskStatus {
    Resolved,
    NoComparableCurrentAsks,
    MarketScopeUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonScope {
    NonSeason,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    Normal,
    Hardcore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MarketScope {
    pub season: SeasonScope,
    pub mode: PlayMode,
}

impl Default for MarketScope {
    fn default() -> Self {
        Self {
            season: SeasonScope::NonSeason,
            mode: PlayMode::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceType {
    CurrentAsks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Dd373,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "ask_basis", rename_all = "snake_case")]
pub enum Pricing {
    PerItem {
        unit_price: AskStatistics,
        entry_price: AskStatistics,
        offers_at_minimum_unit_price: Vec<RuneOffer>,
        offers_at_minimum_entry_price: Vec<RuneOffer>,
    },
    PerListing {
        listing_price: AskStatistics,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AskStatistics {
    #[serde(with = "rust_decimal::serde::str_option")]
    pub minimum: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub median: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RuneOffer {
    pub quantity_per_lot: u64,
    #[serde(with = "rust_decimal::serde::str")]
    pub lot_price: Decimal,
    pub available_lots: u64,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit_price: Decimal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExclusionCounts {
    pub privacy: usize,
    pub multi_item: usize,
    pub unmatched_item: usize,
    pub duplicate_listing: usize,
    pub invalid_offer: usize,
}

/// Bounded errors: no response bodies, listing titles, URLs, or seller details.
#[derive(Debug, Error)]
pub enum MarketError {
    #[error("输入无效：{0}")]
    InvalidInput(&'static str),
    #[error("目录文件无效")]
    InvalidCatalog,
    #[error("网络连接失败（未重试）")]
    Network,
    #[error("网络请求超时（未重试）")]
    Timeout,
    #[error("上游返回 HTTP {0}（未重试）")]
    Http(u16),
    #[error("上游返回了访问验证或登录页面")]
    Challenge,
    #[error("上游响应类型无效")]
    ContentType,
    #[error("上游响应超过 2 MiB")]
    BodyTooLarge,
    #[error("上游 JSON 响应无效")]
    InvalidJson,
    #[error("上游分类结构不唯一")]
    Taxonomy,
    #[error("当前挂单价格字段无效")]
    Price,
    #[error("上游符文挂单价格关系矛盾")]
    OfferRatio,
    #[error("单次查询超过 16 个请求")]
    RequestLimit,
}

impl MarketError {
    pub fn is_invalid_input(&self) -> bool {
        matches!(self, Self::InvalidInput(_) | Self::InvalidCatalog)
    }
}

/// Concrete DD373 implementation. It deliberately exposes no provider trait.
pub struct Dd373CurrentAskLookup {
    catalog: Catalog,
    client: Client,
}

impl Dd373CurrentAskLookup {
    pub fn from_catalog_path(path: impl AsRef<Path>) -> Result<Self, MarketError> {
        let catalog = Catalog::read(path.as_ref()).map_err(|_| MarketError::InvalidCatalog)?;
        let client = build_client()?;
        Ok(Self { catalog, client })
    }

    pub fn lookup(
        &self,
        item: &CanonicalItemId,
        market_scope: MarketScope,
    ) -> Result<CurrentAskSummary, MarketError> {
        let family = self.admit(item)?;
        let mut transport = ReqwestTransport {
            client: &self.client,
        };
        let mut clock = SystemClock;
        lookup_with(
            &self.catalog,
            item,
            family,
            market_scope,
            &mut transport,
            &mut clock,
        )
    }

    fn admit(&self, item: &CanonicalItemId) -> Result<Family, MarketError> {
        match item.kind {
            ItemKind::Base if rune_number(item).is_some() => Ok(Family::Rune),
            ItemKind::Unique | ItemKind::SetItem => {
                let candidates = match item.kind {
                    ItemKind::Unique => self.catalog.unique_candidates(),
                    ItemKind::SetItem => self.catalog.set_candidates(),
                    _ => unreachable!(),
                };
                if self.catalog.canonical_ids().contains(item)
                    && candidates.iter().any(|c| c.id() == item)
                {
                    Ok(if item.kind == ItemKind::Unique {
                        Family::Unique
                    } else {
                        Family::Set
                    })
                } else {
                    Err(MarketError::InvalidInput(
                        "目录中没有可唯一匹配的暗金或套装物品",
                    ))
                }
            }
            _ => Err(MarketError::InvalidInput(
                "仅支持 1 至 33 号符文及目录中的暗金或套装物品",
            )),
        }
    }
}

fn build_client() -> Result<Client, MarketError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| MarketError::Network)
}

#[derive(Clone, Copy)]
enum Family {
    Rune,
    Unique,
    Set,
}

fn rune_number(item: &CanonicalItemId) -> Option<u8> {
    let digits = item.source_key.strip_prefix('r')?;
    if digits.len() != 2 {
        return None;
    }
    let number = digits.parse().ok()?;
    (1..=33).contains(&number).then_some(number)
}

struct RawResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

trait Transport {
    fn get(&mut self, url: &str) -> Result<RawResponse, MarketError>;
}
trait Clock {
    fn now_millis(&mut self) -> u64;
    fn sleep(&mut self, duration: Duration);
}

struct ReqwestTransport<'a> {
    client: &'a Client,
}
impl Transport for ReqwestTransport<'_> {
    fn get(&mut self, url: &str) -> Result<RawResponse, MarketError> {
        if !is_allowed_dd373_url(url) {
            return Err(MarketError::Network);
        }
        let mut response = self.client.get(url).send().map_err(|error| {
            if error.is_timeout() {
                MarketError::Timeout
            } else {
                MarketError::Network
            }
        })?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let body = read_bounded(&mut response)?;
        Ok(RawResponse {
            status,
            content_type,
            body,
        })
    }
}

fn is_allowed_dd373_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && matches!(url.host_str(), Some("game.dd373.com" | "goods.dd373.com"))
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn read_bounded(reader: &mut impl Read) -> Result<Vec<u8>, MarketError> {
    let mut body = Vec::new();
    reader
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| MarketError::Network)?;
    if body.len() > MAX_BODY_BYTES {
        Err(MarketError::BodyTooLarge)
    } else {
        Ok(body)
    }
}

fn is_json_content_type(value: &str) -> bool {
    value
        .split_once(';')
        .map_or(value, |(media_type, _)| media_type)
        .trim()
        .eq_ignore_ascii_case("application/json")
}

struct SystemClock;
impl Clock for SystemClock {
    fn now_millis(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

struct Session<'a> {
    transport: &'a mut dyn Transport,
    clock: &'a mut dyn Clock,
    requests: usize,
    last_start: Option<u64>,
}

impl Session<'_> {
    fn json(&mut self, url: &str) -> Result<Value, MarketError> {
        if self.requests == MAX_REQUESTS {
            return Err(MarketError::RequestLimit);
        }
        let now = self.clock.now_millis();
        if let Some(last) = self.last_start {
            let elapsed = now.saturating_sub(last);
            if elapsed < REQUEST_INTERVAL_MS {
                self.clock
                    .sleep(Duration::from_millis(REQUEST_INTERVAL_MS - elapsed));
            }
        }
        self.last_start = Some(self.clock.now_millis());
        self.requests += 1;
        let response = self.transport.get(url)?;
        if response.status != 200 {
            return Err(MarketError::Http(response.status));
        }
        if !is_json_content_type(&response.content_type) {
            return Err(MarketError::ContentType);
        }
        if response.body.is_empty() {
            return Err(MarketError::InvalidJson);
        }
        let sample = String::from_utf8_lossy(&response.body).to_ascii_lowercase();
        if [
            "captcha",
            "访问验证",
            "安全验证",
            "请登录",
            "browser challenge",
            "cloudflare ray",
        ]
        .iter()
        .any(|m| sample.contains(m))
        {
            return Err(MarketError::Challenge);
        }
        serde_json::from_slice(&response.body).map_err(|_| MarketError::InvalidJson)
    }
}

fn lookup_with(
    catalog: &Catalog,
    item: &CanonicalItemId,
    family: Family,
    market_scope: MarketScope,
    transport: &mut dyn Transport,
    clock: &mut dyn Clock,
) -> Result<CurrentAskSummary, MarketError> {
    let mut session = Session {
        transport,
        clock,
        requests: 0,
        last_start: None,
    };
    let game = session.json("https://game.dd373.com/api/game/list")?;
    let game_id = exact_id(&game, GAME_LABEL)?;
    validate_game(&game, &game_id)?;
    let roots = session.json(&format!(
        "https://game.dd373.com/Api/GameGoodsType/List?parentId={game_id}"
    ))?;
    let areas = session.json(&format!(
        "https://game.dd373.com/Api/GameOther/List?parentId={game_id}"
    ))?;
    let Some(area_id) = area_id(&areas, market_scope.season)? else {
        return Ok(unavailable_summary(
            item,
            family,
            market_scope,
            session.requests,
            session.clock.now_millis(),
        ));
    };
    let servers = session.json(&format!(
        "https://game.dd373.com/Api/GameOther/List?parentId={area_id}"
    ))?;
    let Some(server_id) = server_id(&servers, market_scope)? else {
        return Ok(unavailable_summary(
            item,
            family,
            market_scope,
            session.requests,
            session.clock.now_millis(),
        ));
    };
    let realm_path = format!("{area_id}_{server_id}");
    let root_label = match family {
        Family::Rune => "符文",
        Family::Unique => "暗金装备&饰品",
        Family::Set => "套装",
    };
    let root_id = exact_id(&roots, root_label)?;
    let children = session.json(&format!(
        "https://game.dd373.com/Api/GameGoodsType/List?parentId={root_id}"
    ))?;
    let leaves = leaves_for(&children, family, item)?;
    let candidates: &[Candidate] = match family {
        Family::Unique => catalog.unique_candidates(),
        Family::Set => catalog.set_candidates(),
        Family::Rune => &[],
    };
    let mut records = Vec::new();
    for leaf in leaves {
        let page = session.json(&format!("https://goods.dd373.com/Api/Goods/UserCenter/ApiGetShopList?gameid={game_id}&GameOtherId={realm_path}&GameShopTypeId={leaf}"))?;
        records.extend(listing_records(&page)?);
    }
    summarize(
        item,
        family,
        candidates,
        records,
        market_scope,
        session.requests,
        session.clock.now_millis(),
    )
}

fn objects_with_names<'a>(value: &'a Value, out: &mut Vec<(String, String, &'a Value)>) {
    match value {
        Value::Object(map) => {
            let name = map
                .get("Name")
                .or_else(|| map.get("name"))
                .and_then(Value::as_str);
            let id = map
                .get("Id")
                .or_else(|| map.get("id"))
                .and_then(Value::as_str);
            if let (Some(name), Some(id)) = (name, id) {
                out.push((name.to_owned(), id.to_owned(), value));
            }
            for child in map.values() {
                objects_with_names(child, out);
            }
        }
        Value::Array(values) => {
            for child in values {
                objects_with_names(child, out);
            }
        }
        _ => {}
    }
}

fn taxonomic_objects_with_names<'a>(
    value: &'a Value,
    out: &mut Vec<(String, String, &'a Value)>,
) -> Result<(), MarketError> {
    match value {
        Value::Object(map) => {
            let has_name_key = map.contains_key("Name") || map.contains_key("name");
            let has_id_key = map.contains_key("Id") || map.contains_key("id");
            if has_name_key || has_id_key {
                let name = match (map.get("Name"), map.get("name")) {
                    (Some(name), Some(name_alias)) => {
                        let name = name.as_str().ok_or(MarketError::Taxonomy)?;
                        let name_alias = name_alias.as_str().ok_or(MarketError::Taxonomy)?;
                        if name != name_alias {
                            return Err(MarketError::Taxonomy);
                        }
                        name.to_owned()
                    }
                    (Some(name), None) => name.as_str().ok_or(MarketError::Taxonomy)?.to_owned(),
                    (None, Some(name)) => name.as_str().ok_or(MarketError::Taxonomy)?.to_owned(),
                    (None, None) => return Err(MarketError::Taxonomy),
                };

                let id = match (map.get("Id"), map.get("id")) {
                    (Some(id), Some(id_alias)) => {
                        let id = id.as_str().ok_or(MarketError::Taxonomy)?;
                        let id_alias = id_alias.as_str().ok_or(MarketError::Taxonomy)?;
                        if id != id_alias {
                            return Err(MarketError::Taxonomy);
                        }
                        id.to_owned()
                    }
                    (Some(id), None) => id.as_str().ok_or(MarketError::Taxonomy)?.to_owned(),
                    (None, Some(id)) => id.as_str().ok_or(MarketError::Taxonomy)?.to_owned(),
                    (None, None) => return Err(MarketError::Taxonomy),
                };

                out.push((name, id, value));
            }
            for child in map.values() {
                taxonomic_objects_with_names(child, out)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                taxonomic_objects_with_names(child, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn exact_id(value: &Value, label: &str) -> Result<String, MarketError> {
    let mut objects = Vec::new();
    objects_with_names(value, &mut objects);
    let ids: BTreeSet<_> = objects
        .into_iter()
        .filter(|(name, _, _)| name == label)
        .map(|(_, id, _)| id)
        .collect();
    if ids.len() == 1 {
        Ok(ids.into_iter().next().unwrap())
    } else {
        Err(MarketError::Taxonomy)
    }
}

fn validate_game(value: &Value, id: &str) -> Result<(), MarketError> {
    let mut objects = Vec::new();
    objects_with_names(value, &mut objects);
    let rows: Vec<_> = objects
        .into_iter()
        .filter(|(name, row_id, _)| name == GAME_LABEL && row_id == id)
        .collect();
    if rows.len() != 1 {
        return Err(MarketError::Taxonomy);
    }
    let map = rows[0].2.as_object().ok_or(MarketError::Taxonomy)?;
    let false_or_missing =
        |a: &str, b: &str| map.get(a).or_else(|| map.get(b)).and_then(Value::as_bool) != Some(true);
    let true_or_missing = |a: &str, b: &str| {
        map.get(a).or_else(|| map.get(b)).and_then(Value::as_bool) != Some(false)
    };
    if false_or_missing("IsClose", "isClose")
        && true_or_missing("CanTrade", "canTrade")
        && true_or_missing("IsEnabled", "isEnabled")
    {
        Ok(())
    } else {
        Err(MarketError::Taxonomy)
    }
}

fn valid_taxonomy_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn area_id(value: &Value, season: SeasonScope) -> Result<Option<String>, MarketError> {
    let mut objects = Vec::new();
    taxonomic_objects_with_names(value, &mut objects)?;

    let mut ids = BTreeSet::new();
    let mut has_row = false;
    for (name, id, _) in objects {
        has_row = true;
        let is_area_label = matches!(name.as_str(), "非赛季" | "新赛季" | "赛季");
        if !is_area_label {
            return Err(MarketError::Taxonomy);
        }
        if !valid_taxonomy_id(&id) {
            return Err(MarketError::Taxonomy);
        }
        if matches!(
            (season, name.as_str()),
            (SeasonScope::NonSeason, "非赛季")
                | (SeasonScope::Latest, "新赛季")
                | (SeasonScope::Latest, "赛季")
        ) {
            ids.insert(id);
        }
    }
    if !has_row {
        return Err(MarketError::Taxonomy);
    }
    if ids.len() > 1 {
        Err(MarketError::Taxonomy)
    } else {
        Ok(ids.into_iter().next())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ServerKind {
    Supported(MarketScope),
    Legacy(MarketScope),
}

fn server_kind(label: &str) -> Option<ServerKind> {
    let (kind, season, mode) = match label {
        "非赛季(术士君临)" => (true, SeasonScope::NonSeason, PlayMode::Normal),
        "非赛季专家(术士君临)" => (true, SeasonScope::NonSeason, PlayMode::Hardcore),
        "新赛季(术士君临)" | "赛季(术士君临)" => {
            (true, SeasonScope::Latest, PlayMode::Normal)
        }
        "新赛季专家(术士君临)" | "赛季专家(术士君临)" => {
            (true, SeasonScope::Latest, PlayMode::Hardcore)
        }
        "非赛季普通" => (false, SeasonScope::NonSeason, PlayMode::Normal),
        "非赛季专家" => (false, SeasonScope::NonSeason, PlayMode::Hardcore),
        "新赛季普通" | "赛季普通" => (false, SeasonScope::Latest, PlayMode::Normal),
        "新赛季专家" | "赛季专家" => (false, SeasonScope::Latest, PlayMode::Hardcore),
        _ => return None,
    };
    let scope = MarketScope { season, mode };
    Some(if kind {
        ServerKind::Supported(scope)
    } else {
        ServerKind::Legacy(scope)
    })
}

fn server_id(value: &Value, scope: MarketScope) -> Result<Option<String>, MarketError> {
    let mut objects = Vec::new();
    taxonomic_objects_with_names(value, &mut objects)?;
    let mut ids = BTreeSet::new();
    let mut has_row = false;

    for (name, id, _) in objects {
        has_row = true;
        let Some(kind) = server_kind(&name) else {
            return Err(MarketError::Taxonomy);
        };
        let row_scope = match kind {
            ServerKind::Supported(row_scope) | ServerKind::Legacy(row_scope) => row_scope,
        };
        if !valid_taxonomy_id(&id) || row_scope.season != scope.season {
            return Err(MarketError::Taxonomy);
        }
        if matches!(kind, ServerKind::Supported(row_scope) if row_scope == scope) {
            ids.insert(id);
        }
    }
    if !has_row {
        return Err(MarketError::Taxonomy);
    }

    if ids.len() > 1 {
        return Err(MarketError::Taxonomy);
    }
    Ok(ids.into_iter().next())
}

fn leaves_for(
    value: &Value,
    family: Family,
    item: &CanonicalItemId,
) -> Result<Vec<String>, MarketError> {
    let mut objects = Vec::new();
    objects_with_names(value, &mut objects);
    let pairs: BTreeSet<_> = objects
        .into_iter()
        .map(|(name, id, _)| (name, id))
        .collect();
    match family {
        Family::Rune => {
            let regex = Regex::new(r"^([0-9]+)号符文$").unwrap();
            let mut by_number = BTreeMap::new();
            for (name, id) in pairs {
                if let Some(caps) = regex.captures(&name) {
                    let number: u8 = caps[1].parse().map_err(|_| MarketError::Taxonomy)?;
                    if by_number.insert(number, id).is_some() {
                        return Err(MarketError::Taxonomy);
                    }
                }
            }
            if by_number.len() != 33 || !(1..=33).all(|n| by_number.contains_key(&n)) {
                return Err(MarketError::Taxonomy);
            }
            Ok(vec![by_number.remove(&rune_number(item).unwrap()).unwrap()])
        }
        Family::Unique | Family::Set => {
            if pairs.len() != 9
                || (matches!(family, Family::Set) && !pairs.iter().any(|(name, _)| name == "术士"))
            {
                return Err(MarketError::Taxonomy);
            }
            let ids: BTreeSet<_> = pairs.into_iter().map(|(_, id)| id).collect();
            if ids.len() != 9 || ids.iter().any(String::is_empty) {
                return Err(MarketError::Taxonomy);
            }
            Ok(ids.into_iter().collect())
        }
    }
}

fn listing_records(value: &Value) -> Result<Vec<Value>, MarketError> {
    if let Some(code) = value.get("StatusCode")
        && !is_zero(code)
    {
        return Err(MarketError::InvalidJson);
    }
    let status = value
        .get("StatusData")
        .and_then(Value::as_object)
        .ok_or(MarketError::InvalidJson)?;
    if let Some(code) = status.get("ResultCode")
        && !is_zero(code)
    {
        return Err(MarketError::InvalidJson);
    }
    status
        .get("ResultData")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(MarketError::InvalidJson)
}

fn is_zero(value: &Value) -> bool {
    value.as_i64() == Some(0) || value.as_str() == Some("0")
}

fn summarize(
    item: &CanonicalItemId,
    family: Family,
    candidates: &[Candidate],
    records: Vec<Value>,
    market_scope: MarketScope,
    request_count: usize,
    observed_ms: u64,
) -> Result<CurrentAskSummary, MarketError> {
    let privacy = privacy_regexes();
    let mut exclusions = ExclusionCounts::default();
    let mut seen = BTreeSet::new();
    let mut rune_offers = Vec::new();
    let mut listing_prices = Vec::new();
    let mut listing_count = 0;
    for record in records {
        let object = record.as_object().ok_or(MarketError::InvalidJson)?;
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .ok_or(MarketError::InvalidJson)?;
        if privacy.iter().any(|regex| regex.is_match(title)) {
            exclusions.privacy += 1;
            continue;
        }
        if !matches!(family, Family::Rune) {
            let normalized = normalize_catalog_name(title);
            let ids: BTreeSet<_> = candidates
                .iter()
                .filter(|candidate| normalized.contains(candidate.normalized_name()))
                .map(|candidate| candidate.id())
                .collect();
            if ids.len() > 1 {
                exclusions.multi_item += 1;
                continue;
            }
            if ids.len() != 1 || !ids.contains(item) {
                exclusions.unmatched_item += 1;
                continue;
            }
        }
        let shopno = object
            .get("shopno")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .ok_or(MarketError::InvalidJson)?;
        if !seen.insert(shopno) {
            exclusions.duplicate_listing += 1;
            continue;
        }
        listing_count += 1;
        match family {
            Family::Rune => match parse_rune_offer(object)? {
                Some(offer) => rune_offers.push(offer),
                None => exclusions.invalid_offer += 1,
            },
            Family::Unique | Family::Set => match positive_decimal(object.get("price")) {
                Some(price) => listing_prices.push(price),
                None => exclusions.invalid_offer += 1,
            },
        }
    }
    let (status, pricing, sample_count) = match family {
        Family::Rune => summarize_rune_offers(rune_offers),
        Family::Unique | Family::Set => summarize_listing_prices(listing_prices),
    };
    Ok(summary(
        item,
        market_scope,
        status,
        pricing,
        sample_count,
        listing_count,
        exclusions,
        request_count,
        observed_ms,
    ))
}

fn unavailable_summary(
    item: &CanonicalItemId,
    family: Family,
    market_scope: MarketScope,
    request_count: usize,
    observed_ms: u64,
) -> CurrentAskSummary {
    summary(
        item,
        market_scope,
        CurrentAskStatus::MarketScopeUnavailable,
        empty_pricing(family),
        0,
        0,
        ExclusionCounts::default(),
        request_count,
        observed_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn summary(
    item: &CanonicalItemId,
    market_scope: MarketScope,
    status: CurrentAskStatus,
    pricing: Pricing,
    sample_count: usize,
    listing_count: usize,
    exclusions: ExclusionCounts,
    request_count: usize,
    observed_ms: u64,
) -> CurrentAskSummary {
    CurrentAskSummary {
        schema_version: 3,
        item_id: item.clone(),
        market_scope,
        status,
        price_type: PriceType::CurrentAsks,
        provider: Provider::Dd373,
        currency: CURRENCY,
        pricing,
        sample_count,
        listing_count,
        exclusions,
        request_count,
        observed_at: rfc3339(observed_ms / 1000),
    }
}

fn empty_statistics() -> AskStatistics {
    AskStatistics {
        minimum: None,
        median: None,
    }
}

fn empty_pricing(family: Family) -> Pricing {
    match family {
        Family::Rune => Pricing::PerItem {
            unit_price: empty_statistics(),
            entry_price: empty_statistics(),
            offers_at_minimum_unit_price: Vec::new(),
            offers_at_minimum_entry_price: Vec::new(),
        },
        Family::Unique | Family::Set => Pricing::PerListing {
            listing_price: empty_statistics(),
        },
    }
}

fn positive_decimal(value: Option<&Value>) -> Option<Decimal> {
    parse_decimal(value?)
        .ok()
        .filter(|value| *value > Decimal::ZERO)
}

fn positive_u64(value: Option<&Value>) -> Option<u64> {
    let value = positive_decimal(value)?;
    if !value.fract().is_zero() {
        return None;
    }
    value.to_u64().filter(|value| *value > 0)
}

fn parse_rune_offer(
    object: &serde_json::Map<String, Value>,
) -> Result<Option<RuneOffer>, MarketError> {
    let Some(lot_price) = positive_decimal(object.get("price")) else {
        return Ok(None);
    };
    let Some(unit_price) = positive_decimal(object.get("singleprice")) else {
        return Ok(None);
    };
    let Some(quantity_per_lot) = positive_u64(object.get("amount")) else {
        return Ok(None);
    };
    let Some(available_lots) = positive_u64(object.get("number")) else {
        return Ok(None);
    };
    let calculated = lot_price / Decimal::from(quantity_per_lot);
    let denominator = calculated.abs().max(unit_price.abs());
    if (calculated - unit_price).abs() > RATIO_TOLERANCE * denominator {
        return Err(MarketError::OfferRatio);
    }
    Ok(Some(RuneOffer {
        quantity_per_lot,
        lot_price,
        available_lots,
        unit_price,
    }))
}

fn statistics(values: &mut [Decimal]) -> Option<AskStatistics> {
    if values.is_empty() {
        return None;
    }
    values.sort();
    let middle = values.len() / 2;
    let median = if values.len() % 2 == 1 {
        values[middle]
    } else {
        let lower = values[middle - 1];
        let upper = values[middle];
        lower + (upper - lower) / Decimal::from(2)
    };
    Some(AskStatistics {
        minimum: Some(values[0]),
        median: Some(median),
    })
}

fn summarize_listing_prices(mut prices: Vec<Decimal>) -> (CurrentAskStatus, Pricing, usize) {
    let sample_count = prices.len();
    let listing_price = statistics(&mut prices).unwrap_or_else(empty_statistics);
    let status = if sample_count == 0 {
        CurrentAskStatus::NoComparableCurrentAsks
    } else {
        CurrentAskStatus::Resolved
    };
    (status, Pricing::PerListing { listing_price }, sample_count)
}

fn offer_order(offer: &RuneOffer) -> (Decimal, Decimal, u64, u64) {
    (
        offer.unit_price,
        offer.lot_price,
        offer.quantity_per_lot,
        offer.available_lots,
    )
}

fn summarize_rune_offers(mut offers: Vec<RuneOffer>) -> (CurrentAskStatus, Pricing, usize) {
    let sample_count = offers.len();
    if offers.is_empty() {
        return (
            CurrentAskStatus::NoComparableCurrentAsks,
            empty_pricing(Family::Rune),
            0,
        );
    }
    let mut units: Vec<_> = offers.iter().map(|offer| offer.unit_price).collect();
    let mut entries: Vec<_> = offers.iter().map(|offer| offer.lot_price).collect();
    let unit_price = statistics(&mut units).unwrap();
    let entry_price = statistics(&mut entries).unwrap();
    offers.sort_by_key(offer_order);
    let mut minimum_unit: Vec<_> = offers
        .iter()
        .copied()
        .filter(|offer| Some(offer.unit_price) == unit_price.minimum)
        .collect();
    let mut minimum_entry: Vec<_> = offers
        .iter()
        .copied()
        .filter(|offer| Some(offer.lot_price) == entry_price.minimum)
        .collect();
    minimum_unit.dedup();
    minimum_entry.dedup();
    (
        CurrentAskStatus::Resolved,
        Pricing::PerItem {
            unit_price,
            entry_price,
            offers_at_minimum_unit_price: minimum_unit,
            offers_at_minimum_entry_price: minimum_entry,
        },
        sample_count,
    )
}

fn parse_decimal(value: &Value) -> Result<Decimal, MarketError> {
    match value {
        Value::String(text) => Decimal::from_str_exact(text).map_err(|_| MarketError::Price),
        Value::Number(number) => {
            Decimal::from_str_exact(&number.to_string()).map_err(|_| MarketError::Price)
        }
        _ => Err(MarketError::Price),
    }
}

fn privacy_regexes() -> Vec<Regex> {
    [
        r"[[:alnum:]._%+-]+@[[:alnum:].-]+\.[[:alpha:]]{2,}",
        r"https?://|www\.",
        r"[0-9０-９]{5,}",
        r"qq|微信|vx|v信|telegram|discord|手机号|电话",
    ]
    .iter()
    .map(|pattern| {
        RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .unwrap()
    })
    .collect()
}

fn rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3600,
        day_seconds / 60 % 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests;
