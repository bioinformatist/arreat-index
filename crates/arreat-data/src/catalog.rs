use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    fs::{File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CanonicalItemId, Error, ItemKind, Locale, Result, SCHEMA_VERSION, Snapshot, audit_snapshot,
    error, normalize_to_path,
};

#[cfg(target_os = "linux")]
use crate::exporter::{export_archive_from_root, read_build_info};

pub const NAME_CATALOG_VERSION: u32 = 1;
const OPENCC_VERSION: &str = "1.3.0";
const OPENCC_CONFIG: &str = "tw2s.json";
const ALIAS_MAP_VERSION: u32 = 1;
const ALIAS_PROVENANCE: &str = "bounded_dd373_observation_2026-08-22";
const CACHE_DIRECTORY: &str = "name-catalog-v1";
const EMBEDDED_ALIASES: &[u8] = include_bytes!("../resources/dd373-name-aliases-v1.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogSourceIdentity {
    snapshot_schema_version: u32,
    build_info_sha256: String,
    alias_map_version: u32,
    alias_count: usize,
    alias_sha256: String,
    opencc_version: String,
    opencc_config: String,
    cache_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameCandidate {
    id: CanonicalItemId,
    normalized_name: String,
    source: String,
}

impl NameCandidate {
    pub fn id(&self) -> &CanonicalItemId {
        &self.id
    }

    pub fn normalized_name(&self) -> &str {
        &self.normalized_name
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NameCandidateGroups {
    unique: Vec<NameCandidate>,
    set: Vec<NameCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameCatalog {
    catalog_version: u32,
    source_identity: CatalogSourceIdentity,
    canonical_ids: Vec<CanonicalItemId>,
    candidate_groups: NameCandidateGroups,
}

impl NameCatalog {
    pub(crate) fn build(
        snapshot: &Snapshot,
        alias_bytes: &[u8],
        converted_zh_tw: &[String],
        source_identity: CatalogSourceIdentity,
    ) -> Result<Self> {
        if snapshot.schema_version != SCHEMA_VERSION {
            return Err(Error::Message(format!(
                "name catalog requires snapshot schema {SCHEMA_VERSION}"
            )));
        }
        let audit = audit_snapshot(snapshot);
        if !audit.passed {
            return Err(Error::Message(
                "name catalog requires a snapshot that passes integrity audit".to_owned(),
            ));
        }

        let aliases = AliasMap::parse(alias_bytes)?;
        if source_identity
            != CatalogSourceIdentity::new(&[], alias_bytes)?.with_build_hash(
                source_identity.build_info_sha256.clone(),
                source_identity.cache_key.clone(),
            )
        {
            return Err(Error::Message(
                "catalog source identity does not match alias resources".to_owned(),
            ));
        }

        let known_items = snapshot
            .canonical_items
            .iter()
            .map(|item| item.id.clone())
            .collect::<BTreeSet<_>>();
        aliases.validate_targets(&known_items)?;

        let traditional = snapshot
            .canonical_items
            .iter()
            .flat_map(|item| {
                item.names
                    .iter()
                    .filter(|name| name.locale == Locale::ZhTw)
                    .map(move |name| (&item.id, name.text.as_str()))
            })
            .collect::<Vec<_>>();
        if traditional.len() != converted_zh_tw.len() {
            return Err(Error::Message(format!(
                "OpenCC output cardinality changed: expected {}, found {}",
                traditional.len(),
                converted_zh_tw.len()
            )));
        }
        for (_, text) in &traditional {
            reject_line_break(text, "zhTW source name")?;
        }
        for text in converted_zh_tw {
            reject_line_break(text, "OpenCC converted name")?;
        }

        let mut candidates = Vec::new();
        for item in &snapshot.canonical_items {
            if !matches!(item.id.kind, ItemKind::Unique | ItemKind::SetItem) {
                continue;
            }
            for name in &item.names {
                push_candidate(&mut candidates, &item.id, &name.text, "official");
            }
        }
        for ((id, _), converted) in traditional.iter().zip(converted_zh_tw) {
            if matches!(id.kind, ItemKind::Unique | ItemKind::SetItem) {
                push_candidate(&mut candidates, id, converted, "opencc");
            }
        }
        for alias in &aliases.entries {
            push_candidate(
                &mut candidates,
                &alias.canonical_id,
                &alias.alias,
                "community",
            );
        }
        candidates.sort_by(candidate_order);
        candidates.dedup();

        let unique = candidates
            .iter()
            .filter(|candidate| candidate.id.kind == ItemKind::Unique)
            .cloned()
            .collect::<Vec<_>>();
        let set = candidates
            .into_iter()
            .filter(|candidate| candidate.id.kind == ItemKind::SetItem)
            .collect::<Vec<_>>();
        let mut canonical_ids = (1..=33)
            .map(|number| CanonicalItemId {
                kind: ItemKind::Base,
                source_key: format!("r{number:02}"),
            })
            .chain(unique.iter().map(|candidate| candidate.id.clone()))
            .chain(set.iter().map(|candidate| candidate.id.clone()))
            .collect::<Vec<_>>();
        canonical_ids.sort_by_key(ToString::to_string);
        canonical_ids.dedup();

        let catalog = Self {
            catalog_version: NAME_CATALOG_VERSION,
            source_identity,
            canonical_ids,
            candidate_groups: NameCandidateGroups { unique, set },
        };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let catalog: Self =
            serde_json::from_reader(File::open(path).map_err(|source| error::io(path, source))?)
                .map_err(|source| Error::Json {
                    path: path.display().to_string(),
                    source,
                })?;
        catalog.validate()?;
        Ok(catalog)
    }

    fn write_atomic(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|source| Error::Json {
            path: path.display().to_string(),
            source,
        })?;
        bytes.push(b'\n');
        atomic_write(path, &bytes)
    }

    pub fn validate(&self) -> Result<()> {
        if self.catalog_version != NAME_CATALOG_VERSION {
            return invalid_catalog("unsupported catalog version");
        }
        self.source_identity.validate()?;
        if self.canonical_ids.is_empty()
            || !strictly_sorted_by(&self.canonical_ids, |id| id.to_string())
        {
            return invalid_catalog("canonical IDs must be nonempty, sorted, and unique");
        }
        let ids = self.canonical_ids.iter().collect::<BTreeSet<_>>();
        for number in 1..=33 {
            let rune = CanonicalItemId {
                kind: ItemKind::Base,
                source_key: format!("r{number:02}"),
            };
            if !ids.contains(&rune) {
                return invalid_catalog("all 33 rune canonical IDs are required");
            }
        }
        if self.canonical_ids.iter().any(|id| {
            matches!(id.kind, ItemKind::Runeword) || (id.kind == ItemKind::Base && !is_rune_id(id))
        }) {
            return invalid_catalog("catalog contains an unsupported canonical ID");
        }
        if self.candidate_groups.unique.is_empty() || self.candidate_groups.set.is_empty() {
            return invalid_catalog("Unique and Set candidate groups must be nonempty");
        }

        let mut candidate_ids = BTreeSet::new();
        for (kind, rows) in [
            (ItemKind::Unique, &self.candidate_groups.unique),
            (ItemKind::SetItem, &self.candidate_groups.set),
        ] {
            if !strictly_sorted_by(rows, |row| {
                (
                    row.id.to_string(),
                    row.normalized_name.clone(),
                    row.source.clone(),
                )
            }) {
                return invalid_catalog("candidate rows must be stably sorted and unique");
            }
            for row in rows {
                if row.id.kind != kind
                    || !ids.contains(&row.id)
                    || row.normalized_name.is_empty()
                    || normalize_catalog_name(&row.normalized_name) != row.normalized_name
                    || !matches!(row.source.as_str(), "official" | "opencc" | "community")
                {
                    return invalid_catalog("candidate row is malformed or has the wrong family");
                }
                candidate_ids.insert(&row.id);
            }
        }
        if self
            .canonical_ids
            .iter()
            .filter(|id| matches!(id.kind, ItemKind::Unique | ItemKind::SetItem))
            .any(|id| !candidate_ids.contains(id))
        {
            return invalid_catalog("a fixed-name canonical ID has no candidate row");
        }
        Ok(())
    }

    pub fn canonical_ids(&self) -> &[CanonicalItemId] {
        &self.canonical_ids
    }

    pub fn unique_candidates(&self) -> &[NameCandidate] {
        &self.candidate_groups.unique
    }

    pub fn set_candidates(&self) -> &[NameCandidate] {
        &self.candidate_groups.set
    }

    pub(crate) fn source_identity(&self) -> &CatalogSourceIdentity {
        &self.source_identity
    }
}

impl CatalogSourceIdentity {
    fn new(build_info: &[u8], alias_bytes: &[u8]) -> Result<Self> {
        let aliases = AliasMap::parse(alias_bytes)?;
        let normalized_aliases = aliases.normalized_bytes()?;
        let cache_key = framed_digest(&[
            build_info,
            &NAME_CATALOG_VERSION.to_le_bytes(),
            &normalized_aliases,
            OPENCC_VERSION.as_bytes(),
            OPENCC_CONFIG.as_bytes(),
            &SCHEMA_VERSION.to_le_bytes(),
        ]);
        Ok(Self {
            snapshot_schema_version: SCHEMA_VERSION,
            build_info_sha256: hex_digest(build_info),
            alias_map_version: aliases.version,
            alias_count: aliases.entries.len(),
            alias_sha256: hex_digest(&normalized_aliases),
            opencc_version: OPENCC_VERSION.to_owned(),
            opencc_config: OPENCC_CONFIG.to_owned(),
            cache_key,
        })
    }

    fn with_build_hash(mut self, build_info_sha256: String, cache_key: String) -> Self {
        self.build_info_sha256 = build_info_sha256;
        self.cache_key = cache_key;
        self
    }

    fn validate(&self) -> Result<()> {
        if self.snapshot_schema_version != SCHEMA_VERSION
            || self.alias_map_version != ALIAS_MAP_VERSION
            || self.alias_count == 0
            || self.opencc_version != OPENCC_VERSION
            || self.opencc_config != OPENCC_CONFIG
            || !is_sha256(&self.build_info_sha256)
            || !is_sha256(&self.alias_sha256)
            || !is_sha256(&self.cache_key)
        {
            return invalid_catalog("source identity is invalid");
        }
        Ok(())
    }
}

pub fn normalize_catalog_name(value: &str) -> String {
    static PUNCTUATION_OR_SPACE: OnceLock<Regex> = OnceLock::new();
    let mut mapped = String::with_capacity(value.len());
    for character in value.chars() {
        mapped.push(match character {
            '０' => '0',
            '１' => '1',
            '２' => '2',
            '３' => '3',
            '４' => '4',
            '５' => '5',
            '６' => '6',
            '７' => '7',
            '８' => '8',
            '９' => '9',
            other => other,
        });
    }
    let lower = mapped.to_ascii_lowercase();
    PUNCTUATION_OR_SPACE
        .get_or_init(|| Regex::new(r"[\p{P}\s]").expect("fixed catalog normalization regex"))
        .replace_all(&lower, "")
        .into_owned()
}

pub fn catalog_local_install(game_root: &Path, cache_root: Option<&Path>) -> Result<PathBuf> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (game_root, cache_root);
        Err(Error::UnsupportedPlatform)
    }
    #[cfg(target_os = "linux")]
    {
        let game_root =
            fs::canonicalize(game_root).map_err(|source| error::io(game_root, source))?;
        if !game_root.is_dir() {
            return Err(Error::Message("game root must be a directory".to_owned()));
        }
        let requested_cache = match cache_root {
            Some(path) => path.to_path_buf(),
            None => default_cache_root()?,
        };
        let cache_root = prepare_cache_root(&requested_cache, &game_root)?;
        let build_info = read_build_info(&game_root)?;
        let resources = CatalogResources::new(EMBEDDED_ALIASES, &build_info)?;
        cached_catalog(
            &game_root,
            &cache_root,
            &build_info,
            &resources,
            &mut ProductionMiss,
        )
    }
}

struct CatalogResources<'a> {
    aliases: &'a [u8],
    identity: CatalogSourceIdentity,
}

impl<'a> CatalogResources<'a> {
    fn new(aliases: &'a [u8], build_info: &[u8]) -> Result<Self> {
        Ok(Self {
            aliases,
            identity: CatalogSourceIdentity::new(build_info, aliases)?,
        })
    }
}

trait CacheMiss {
    fn build(
        &mut self,
        game_root: &Path,
        build_info: &[u8],
        staging: &Path,
        resources: &CatalogResources<'_>,
    ) -> Result<NameCatalog>;
}

struct ProductionMiss;

#[cfg(target_os = "linux")]
impl CacheMiss for ProductionMiss {
    fn build(
        &mut self,
        game_root: &Path,
        build_info: &[u8],
        staging: &Path,
        resources: &CatalogResources<'_>,
    ) -> Result<NameCatalog> {
        let archive = staging.join("source.tar");
        let snapshot_path = staging.join("snapshot.json");
        export_archive_from_root(game_root, build_info, &archive)?;
        let snapshot = normalize_to_path(&archive, &snapshot_path)?;
        let converted = convert_zh_tw(&snapshot, staging)?;
        NameCatalog::build(
            &snapshot,
            resources.aliases,
            &converted,
            resources.identity.clone(),
        )
    }
}

fn cached_catalog(
    game_root: &Path,
    cache_root: &Path,
    build_info: &[u8],
    resources: &CatalogResources<'_>,
    miss: &mut dyn CacheMiss,
) -> Result<PathBuf> {
    let schema_root = exact_child_directory(cache_root, CACHE_DIRECTORY)?;
    ensure_disjoint(&schema_root, game_root)?;
    let final_path = schema_root.join(format!("{}.json", resources.identity.cache_key));
    if let Ok(catalog) = NameCatalog::read(&final_path)
        && catalog.source_identity() == &resources.identity
    {
        return Ok(final_path);
    }

    let staging = StagingDirectory::create(cache_root, game_root)?;
    let catalog = miss.build(game_root, build_info, staging.path(), resources)?;
    if catalog.source_identity() != &resources.identity {
        return Err(Error::Message(
            "catalog builder returned the wrong source identity".to_owned(),
        ));
    }
    catalog.write_atomic(&final_path)?;
    let published = NameCatalog::read(&final_path)?;
    if published.source_identity() != &resources.identity {
        return Err(Error::Message(
            "published catalog source identity does not match its cache key".to_owned(),
        ));
    }
    staging.cleanup()?;
    Ok(final_path)
}

fn convert_zh_tw(snapshot: &Snapshot, staging: &Path) -> Result<Vec<String>> {
    require_opencc_version()?;
    convert_zh_tw_with(snapshot, staging, Path::new("opencc"))
}

fn convert_zh_tw_with(
    snapshot: &Snapshot,
    staging: &Path,
    executable: &Path,
) -> Result<Vec<String>> {
    let names = snapshot
        .canonical_items
        .iter()
        .flat_map(|item| &item.names)
        .filter(|name| name.locale == Locale::ZhTw)
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>();
    for name in &names {
        reject_line_break(name, "zhTW source name")?;
    }
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut input = names.join("\n");
    input.push('\n');
    let input_path = staging.join("opencc-input.txt");
    let output_path = staging.join("opencc-output.txt");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut input_file = options
        .open(&input_path)
        .map_err(|source| error::io(&input_path, source))?;
    input_file
        .write_all(input.as_bytes())
        .map_err(|source| error::io(&input_path, source))?;
    input_file
        .sync_all()
        .map_err(|source| error::io(&input_path, source))?;
    drop(input_file);

    let output = Command::new(executable)
        .arg("-c")
        .arg(OPENCC_CONFIG)
        .arg("-i")
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| Error::Message(format!("could not execute OpenCC: {source}")))?;
    if !output.status.success() {
        return Err(Error::Message("OpenCC conversion failed".to_owned()));
    }
    let output_bytes = fs::read(&output_path).map_err(|source| error::io(&output_path, source))?;
    let output = String::from_utf8(output_bytes)
        .map_err(|_| Error::Message("OpenCC output is not UTF-8".to_owned()))?;
    if output.contains('\r') || !output.ends_with('\n') {
        return Err(Error::Message(
            "OpenCC output has an invalid newline shape".to_owned(),
        ));
    }
    let converted = output
        .strip_suffix('\n')
        .unwrap()
        .split('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if converted.len() != names.len() {
        return Err(Error::Message(format!(
            "OpenCC output cardinality changed: expected {}, found {}",
            names.len(),
            converted.len()
        )));
    }
    Ok(converted)
}

fn require_opencc_version() -> Result<()> {
    let output = Command::new("opencc")
        .arg("--version")
        .output()
        .map_err(|source| Error::Message(format!("could not execute OpenCC: {source}")))?;
    if !output.status.success() {
        return Err(Error::Message("OpenCC --version failed".to_owned()));
    }
    let mut bytes = output.stdout;
    bytes.extend(output.stderr);
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::Message("OpenCC version output is not UTF-8".to_owned()))?;
    let versions = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Version: "))
        .collect::<Vec<_>>();
    if versions != [OPENCC_VERSION] {
        return Err(Error::Message(format!(
            "OpenCC {OPENCC_VERSION} required, found {}",
            versions.first().copied().unwrap_or("unknown")
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasMap {
    version: u32,
    entries: Vec<AliasEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasEntry {
    canonical_id: CanonicalItemId,
    alias: String,
    kind: AliasKind,
    provenance: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AliasKind {
    Abbreviation,
    LegacySimplified,
    CommonMisspelling,
    MarketShorthand,
}

impl AliasMap {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let aliases = serde_json::from_slice::<Self>(bytes).map_err(|source| Error::Json {
            path: "embedded DD373 aliases".to_owned(),
            source,
        })?;
        if aliases.version != ALIAS_MAP_VERSION || aliases.entries.is_empty() {
            return Err(Error::Message("unsupported or empty alias map".to_owned()));
        }
        let mut normalized_targets = BTreeMap::new();
        for entry in &aliases.entries {
            if !matches!(
                entry.canonical_id.kind,
                ItemKind::Unique | ItemKind::SetItem
            ) || entry.provenance != ALIAS_PROVENANCE
            {
                return Err(Error::Message(
                    "alias has an unsupported target, kind, or provenance".to_owned(),
                ));
            }
            let normalized = normalize_catalog_name(&entry.alias);
            if normalized.is_empty() {
                return Err(Error::Message(
                    "alias normalizes to an empty name".to_owned(),
                ));
            }
            if let Some(previous) = normalized_targets.insert(normalized, &entry.canonical_id)
                && previous != &entry.canonical_id
            {
                return Err(Error::Message(
                    "one normalized alias maps to multiple canonical IDs".to_owned(),
                ));
            }
        }
        let mut rows = aliases.entries.clone();
        rows.sort();
        if rows.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::Message(
                "alias map contains a duplicate row".to_owned(),
            ));
        }
        Ok(aliases)
    }

    fn validate_targets(&self, known: &BTreeSet<CanonicalItemId>) -> Result<()> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| !known.contains(&entry.canonical_id))
        {
            return Err(Error::Message(format!(
                "alias {:?} references unknown target {}",
                entry.alias, entry.canonical_id
            )));
        }
        Ok(())
    }

    fn normalized_bytes(&self) -> Result<Vec<u8>> {
        let mut normalized = self.clone();
        normalized.entries.sort();
        serde_json::to_vec(&normalized).map_err(|source| Error::Json {
            path: "normalized embedded DD373 aliases".to_owned(),
            source,
        })
    }
}

fn default_cache_root() -> Result<PathBuf> {
    if let Some(value) = env::var_os("XDG_CACHE_HOME") {
        let root = PathBuf::from(value);
        require_absolute_cache_prerequisite("XDG_CACHE_HOME", &root)?;
        return Ok(root.join("arreat-index"));
    }
    let home = env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        Error::Message("set XDG_CACHE_HOME or HOME to an absolute path".to_owned())
    })?;
    require_absolute_cache_prerequisite("HOME", &home)?;
    Ok(home.join(".cache/arreat-index"))
}

fn require_absolute_cache_prerequisite(label: &str, path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(Error::Message(format!(
            "{label} must be an absolute path without '.' or '..' components"
        )));
    }
    Ok(())
}

fn prepare_cache_root(requested: &Path, game_root: &Path) -> Result<PathBuf> {
    require_absolute_cache_prerequisite("cache root", requested)?;
    let projected = projected_canonical_path(requested)?;
    ensure_disjoint(&projected, game_root)?;
    fs::create_dir_all(requested).map_err(|source| error::io(requested, source))?;
    let canonical = fs::canonicalize(requested).map_err(|source| error::io(requested, source))?;
    ensure_disjoint(&canonical, game_root)?;
    if !canonical.is_dir() {
        return Err(Error::Message("cache root must be a directory".to_owned()));
    }
    Ok(canonical)
}

fn projected_canonical_path(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| Error::Message("cache root has no existing ancestor".to_owned()))?;
        missing.push(name.to_owned());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| Error::Message("cache root has no existing ancestor".to_owned()))?;
    }
    let mut projected = fs::canonicalize(ancestor).map_err(|source| error::io(ancestor, source))?;
    for component in missing.iter().rev() {
        projected.push(component);
    }
    Ok(projected)
}

fn ensure_disjoint(left: &Path, right: &Path) -> Result<()> {
    if left.starts_with(right) || right.starts_with(left) {
        return Err(Error::Message(format!(
            "cache paths and game root must not overlap: {} and {}",
            left.display(),
            right.display()
        )));
    }
    Ok(())
}

fn exact_child_directory(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(source) => return Err(error::io(&path, source)),
    }
    let canonical = fs::canonicalize(&path).map_err(|source| error::io(&path, source))?;
    if canonical != path || !canonical.is_dir() {
        return Err(Error::UnsafePath(path.display().to_string()));
    }
    Ok(canonical)
}

struct StagingDirectory {
    path: PathBuf,
    armed: bool,
}

impl StagingDirectory {
    fn create(cache_root: &Path, game_root: &Path) -> Result<Self> {
        for _ in 0..32 {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random)
                .map_err(|source| Error::Message(format!("OS randomness unavailable: {source}")))?;
            let name = format!(".catalog-stage-{}", hex_bytes(&random));
            let path = cache_root.join(name);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&path) {
                Ok(()) => {
                    let canonical =
                        fs::canonicalize(&path).map_err(|source| error::io(&path, source))?;
                    if canonical.parent() != Some(cache_root) {
                        let _ = fs::remove_dir(&canonical);
                        return Err(Error::UnsafePath(path.display().to_string()));
                    }
                    ensure_disjoint(&canonical, game_root)?;
                    return Ok(Self {
                        path: canonical,
                        armed: true,
                    });
                }
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(error::io(&path, source)),
            }
        }
        Err(Error::Message(
            "could not create a unique catalog staging directory".to_owned(),
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup(mut self) -> Result<()> {
        fs::remove_dir_all(&self.path).map_err(|source| error::io(&self.path, source))?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn push_candidate(rows: &mut Vec<NameCandidate>, id: &CanonicalItemId, name: &str, source: &str) {
    let normalized_name = normalize_catalog_name(name);
    if !normalized_name.is_empty() {
        rows.push(NameCandidate {
            id: id.clone(),
            normalized_name,
            source: source.to_owned(),
        });
    }
}

fn candidate_order(left: &NameCandidate, right: &NameCandidate) -> std::cmp::Ordering {
    (&left.id.to_string(), &left.normalized_name, &left.source).cmp(&(
        &right.id.to_string(),
        &right.normalized_name,
        &right.source,
    ))
}

fn strictly_sorted_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn is_rune_id(id: &CanonicalItemId) -> bool {
    id.source_key
        .strip_prefix('r')
        .filter(|digits| digits.len() == 2)
        .and_then(|digits| digits.parse::<u8>().ok())
        .is_some_and(|number| (1..=33).contains(&number))
}

fn reject_line_break(value: &str, label: &str) -> Result<()> {
    if value.contains(['\n', '\r']) {
        return Err(Error::Message(format!("{label} contains a line break")));
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Message("catalog output must have a parent".to_owned()))?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|source| Error::Message(format!("OS randomness unavailable: {source}")))?;
    let temporary = parent.join(format!(".catalog-publish-{}", hex_bytes(&random)));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&temporary)
            .map_err(|source| error::io(&temporary, source))?;
        file.write_all(bytes)
            .map_err(|source| error::io(&temporary, source))?;
        file.sync_all()
            .map_err(|source| error::io(&temporary, source))?;
        fs::rename(&temporary, path).map_err(|source| error::io(path, source))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| error::io(parent, source))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn framed_digest(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hex_bytes(&hash.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn invalid_catalog<T>(reason: &str) -> Result<T> {
    Err(Error::Message(format!("invalid name catalog: {reason}")))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    const FIXTURE_ALIASES: &[u8] = br#"{
      "version":1,
      "entries":[
        {"canonical_id":"unique:Ars Al'Diablolos","alias":"\u5b66\u672f\u522b\u540d","kind":"abbreviation","provenance":"bounded_dd373_observation_2026-08-22"},
        {"canonical_id":"set-item:Modern Valid Set","alias":"\u73b0\u4ee3\u5957\u88c5","kind":"market_shorthand","provenance":"bounded_dd373_observation_2026-08-22"}
      ]
    }"#;

    fn fixture_snapshot() -> Snapshot {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/d2r-minimal");
        let private = temporary_directory("fixture");
        let result = {
            for relative in std::iter::once(".build.info")
                .chain(
                    source
                        .join("aliases.json")
                        .exists()
                        .then_some("aliases.json"),
                )
                .chain(crate::SOURCE_WHITELIST.iter().copied())
            {
                let destination = private.join(relative);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::copy(source.join(relative), destination).unwrap();
            }
            crate::normalize_input(&private)
        };
        fs::remove_dir_all(&private).unwrap();
        result.unwrap()
    }

    fn fixture_converted(snapshot: &Snapshot) -> Vec<String> {
        snapshot
            .canonical_items
            .iter()
            .flat_map(|item| &item.names)
            .filter(|name| name.locale == Locale::ZhTw)
            .map(|name| match name.text.as_str() {
                "舊書" => "旧书".to_owned(),
                "艾迪亞布羅斯學術" => "艾迪亚布罗斯学术".to_owned(),
                other => other.to_owned(),
            })
            .collect()
    }

    fn fixture_catalog(build: &[u8]) -> NameCatalog {
        let snapshot = fixture_snapshot();
        NameCatalog::build(
            &snapshot,
            FIXTURE_ALIASES,
            &fixture_converted(&snapshot),
            CatalogSourceIdentity::new(build, FIXTURE_ALIASES).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn embedded_alias_map_is_the_exact_bounded_87_record_resource() {
        let aliases = AliasMap::parse(EMBEDDED_ALIASES).unwrap();
        assert_eq!(aliases.version, 1);
        assert_eq!(aliases.entries.len(), 87);
        assert!(
            aliases
                .entries
                .iter()
                .all(|entry| entry.provenance == ALIAS_PROVENANCE)
        );
    }

    #[test]
    fn fixture_builder_is_deterministic_and_matches_expected_bytes() {
        let first = fixture_catalog(b"fixture-build");
        let second = fixture_catalog(b"fixture-build");
        let mut actual = serde_json::to_vec_pretty(&first).unwrap();
        actual.push(b'\n');
        assert_eq!(first, second);
        assert_eq!(
            actual,
            include_bytes!("../../../tests/fixtures/catalog/expected-name-catalog.json")
        );
        assert_eq!(
            first
                .canonical_ids()
                .iter()
                .filter(|id| id.kind == ItemKind::Base)
                .count(),
            33
        );
        assert!(
            first
                .canonical_ids()
                .iter()
                .all(|id| !matches!(id.kind, ItemKind::Runeword))
        );
    }

    #[test]
    fn read_accepts_fixture() {
        let expected = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/catalog/expected-name-catalog.json");
        assert!(NameCatalog::read(&expected).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn file_backed_conversion_drains_large_child_output_before_input_is_read() {
        let root = temporary_directory("opencc-file-exchange");
        let script = root.join("fake-opencc");
        fs::write(
            &script,
            br#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in
    -i) input=$2; shift 2 ;;
    -o) output=$2; shift 2 ;;
    *) shift ;;
  esac
done
i=0
while [ "$i" -lt 8192 ]; do
  printf '0123456789abcdef0123456789abcdef'
  i=$((i + 1))
done
while IFS= read -r line; do
  printf '%s\n' "$line"
done < "$input" > "$output"
"#,
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let snapshot = fixture_snapshot();
        let actual = convert_zh_tw_with(&snapshot, &root, &script).unwrap();
        let expected = snapshot
            .canonical_items
            .iter()
            .flat_map(|item| &item.names)
            .filter(|name| name.locale == Locale::ZhTw)
            .map(|name| name.text.clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normalization_and_injected_opencc_sentinels_preserve_all_layers() {
        assert_eq!(normalize_catalog_name("ＴＥＳＴ？ １２"), "ＴＥＳＴ12");
        let sentinels = [
            ("塔拉夏的判決", "塔拉夏的判决"),
            ("吉永之臉", "吉永之脸"),
            ("蛇魔法師之皮", "蛇魔法师之皮"),
            ("馬拉的萬花筒", "马拉的万花筒"),
        ];
        for (traditional, simplified) in sentinels {
            assert_ne!(
                normalize_catalog_name(traditional),
                normalize_catalog_name(simplified)
            );
            assert!(!normalize_catalog_name(simplified).is_empty());
        }
        let catalog = fixture_catalog(b"layers");
        let sources = catalog
            .unique_candidates()
            .iter()
            .map(NameCandidate::source)
            .collect::<BTreeSet<_>>();
        assert_eq!(sources, BTreeSet::from(["community", "official", "opencc"]));
    }

    #[test]
    fn malformed_duplicate_and_wrong_family_catalogs_fail_closed() {
        let catalog = fixture_catalog(b"invalid");
        let mutate = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut value = serde_json::to_value(&catalog).unwrap();
            edit(&mut value);
            serde_json::from_value::<NameCatalog>(value)
                .unwrap()
                .validate()
        };
        assert!(
            mutate(&|value| {
                let duplicate = value["canonical_ids"][0].clone();
                value["canonical_ids"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            })
            .is_err()
        );
        assert!(mutate(&|value| value["catalog_version"] = 99.into()).is_err());
        assert!(
            mutate(&|value| {
                value["candidate_groups"]["unique"][0]["normalized_name"] = "".into();
            })
            .is_err()
        );
        assert!(
            mutate(&|value| {
                value["candidate_groups"]["unique"][0]["id"] = "set-item:Modern Valid Set".into();
            })
            .is_err()
        );
        assert!(
            mutate(&|value| {
                let duplicate = value["candidate_groups"]["unique"][0].clone();
                value["candidate_groups"]["unique"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            })
            .is_err()
        );
    }

    #[test]
    fn candidate_names_may_resolve_to_multiple_canonical_ids() {
        let mut catalog = fixture_catalog(b"cross-id");
        let shared_name = catalog.candidate_groups.unique[0].normalized_name.clone();
        let first_id = catalog.candidate_groups.unique[0].id.clone();
        let other = catalog
            .candidate_groups
            .unique
            .iter_mut()
            .find(|candidate| candidate.id != first_id)
            .unwrap();
        other.normalized_name = shared_name.clone();
        catalog.candidate_groups.unique.sort_by(candidate_order);

        catalog.validate().unwrap();
        assert_eq!(
            catalog
                .unique_candidates()
                .iter()
                .filter(|candidate| candidate.normalized_name() == shared_name)
                .map(NameCandidate::id)
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn aliases_reject_conflicts_unknown_targets_and_bad_provenance() {
        let snapshot = fixture_snapshot();
        let replacement = FIXTURE_ALIASES.to_vec();
        assert!(AliasMap::parse(&replacement).is_ok());
        let conflict = br#"{"version":1,"entries":[
          {"canonical_id":"unique:Ars Al'Diablolos","alias":"same","kind":"abbreviation","provenance":"bounded_dd373_observation_2026-08-22"},
          {"canonical_id":"set-item:Modern Valid Set","alias":"same","kind":"abbreviation","provenance":"bounded_dd373_observation_2026-08-22"}
        ]}"#;
        assert!(AliasMap::parse(conflict).is_err());
        let bad_provenance = br#"{"version":1,"entries":[
          {"canonical_id":"unique:Ars Al'Diablolos","alias":"bad","kind":"abbreviation","provenance":"unreviewed"}
        ]}"#;
        assert!(AliasMap::parse(bad_provenance).is_err());
        let unknown = br#"{"version":1,"entries":[
          {"canonical_id":"unique:missing","alias":"missing","kind":"abbreviation","provenance":"bounded_dd373_observation_2026-08-22"}
        ]}"#;
        assert!(
            AliasMap::parse(unknown)
                .unwrap()
                .validate_targets(
                    &snapshot
                        .canonical_items
                        .iter()
                        .map(|item| item.id.clone())
                        .collect()
                )
                .is_err()
        );
    }

    struct FixtureMiss<'a> {
        calls: &'a AtomicUsize,
        fail: bool,
    }

    impl CacheMiss for FixtureMiss<'_> {
        fn build(
            &mut self,
            _game_root: &Path,
            _build_info: &[u8],
            staging: &Path,
            resources: &CatalogResources<'_>,
        ) -> Result<NameCatalog> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            fs::write(staging.join("archive.tar"), b"private").unwrap();
            fs::write(staging.join("snapshot.json"), b"private").unwrap();
            if self.fail {
                return Err(Error::Message("injected miss failure".to_owned()));
            }
            let snapshot = fixture_snapshot();
            NameCatalog::build(
                &snapshot,
                resources.aliases,
                &fixture_converted(&snapshot),
                resources.identity.clone(),
            )
        }
    }

    #[test]
    fn cache_hit_skips_miss_work_and_identity_changes_invalidate() {
        let root = temporary_directory("cache");
        let game = exact_child_directory(&root, "game").unwrap();
        let cache = exact_child_directory(&root, "cache").unwrap();
        let calls = AtomicUsize::new(0);
        let first_resources = CatalogResources::new(FIXTURE_ALIASES, b"build-a").unwrap();
        let first = cached_catalog(
            &game,
            &cache,
            b"build-a",
            &first_resources,
            &mut FixtureMiss {
                calls: &calls,
                fail: false,
            },
        )
        .unwrap();
        let hit = cached_catalog(
            &game,
            &cache,
            b"build-a",
            &first_resources,
            &mut FixtureMiss {
                calls: &calls,
                fail: true,
            },
        )
        .unwrap();
        assert_eq!(first, hit);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        fs::write(&first, b"{}\n").unwrap();
        let rebuilt = cached_catalog(
            &game,
            &cache,
            b"build-a",
            &first_resources,
            &mut FixtureMiss {
                calls: &calls,
                fail: false,
            },
        )
        .unwrap();
        assert_eq!(first, rebuilt);
        assert!(NameCatalog::read(&rebuilt).is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let changed = CatalogResources::new(FIXTURE_ALIASES, b"build-b").unwrap();
        let second = cached_catalog(
            &game,
            &cache,
            b"build-b",
            &changed,
            &mut FixtureMiss {
                calls: &calls,
                fail: false,
            },
        )
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert!(fs::read_dir(&cache).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("stage")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failure_cleans_staging_and_path_overlap_is_rejected_before_creation() {
        let root = temporary_directory("cleanup");
        let game = exact_child_directory(&root, "game").unwrap();
        let cache = exact_child_directory(&root, "cache").unwrap();
        let calls = AtomicUsize::new(0);
        let resources = CatalogResources::new(FIXTURE_ALIASES, b"failure").unwrap();
        assert!(
            cached_catalog(
                &game,
                &cache,
                b"failure",
                &resources,
                &mut FixtureMiss {
                    calls: &calls,
                    fail: true
                },
            )
            .is_err()
        );
        assert!(fs::read_dir(&cache).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("stage")
        }));
        let inside = game.join("must-not-exist");
        assert!(prepare_cache_root(&inside, &game).is_err());
        assert!(!inside.exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = env::temp_dir().join(format!("arreat-catalog-{label}-{}", hex_bytes(&random)));
        fs::create_dir(&path).unwrap();
        fs::canonicalize(path).unwrap()
    }
}
