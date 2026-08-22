use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Component, Path},
};

use csv::StringRecord;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    error::{self, Error, Result},
    exporter::SOURCE_WHITELIST,
    model::{
        AffixDefinition, AffixKind, AffixModifier, AffixTable, AliasRecord, AuditFinding,
        CanonicalAffixId, CanonicalItem, CanonicalItemId, FindingSeverity, GameBuild, InputDigest,
        ItemKind, Locale, LocalizedName, ModifierInterpretation, SCHEMA_VERSION,
        ScaledChargedSkill, Snapshot, SourceOperands,
    },
};

const BUILD_INFO: &str = ".build.info";
const ALIASES: &str = "aliases.json";
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
struct InputBundle {
    files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone)]
struct Localization {
    values: BTreeMap<String, BTreeMap<Locale, String>>,
}

#[derive(Debug, Clone)]
struct Row {
    number: u32,
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SkillMetadata {
    required_level: u32,
}

impl Row {
    fn has_field(&self, names: &[&str]) -> bool {
        names
            .iter()
            .any(|name| self.fields.contains_key(&name.to_ascii_lowercase()))
    }

    fn get(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|name| {
            self.fields
                .get(&name.to_ascii_lowercase())
                .map(String::as_str)
                .filter(|value| !value.is_empty())
        })
    }

    fn integer(&self, names: &[&str], default: u32) -> Result<u32> {
        match self.get(names) {
            Some(value) => value.parse().map_err(|_| {
                Error::Message(format!(
                    "row {} has invalid integer {value:?} in {}",
                    self.number,
                    names.join("/")
                ))
            }),
            None => Ok(default),
        }
    }
}

pub fn normalize_input(input: &Path) -> Result<Snapshot> {
    let bundle = InputBundle::load(input)?;
    normalize_bundle(bundle)
}

pub fn normalize_to_path(input: &Path, output: &Path) -> Result<Snapshot> {
    let snapshot = normalize_input(input)?;
    let mut bytes = serde_json::to_vec_pretty(&snapshot).map_err(|source| Error::Json {
        path: output.display().to_string(),
        source,
    })?;
    bytes.push(b'\n');
    atomic_write(output, &bytes)?;
    Ok(snapshot)
}

fn normalize_bundle(bundle: InputBundle) -> Result<Snapshot> {
    let build = parse_build(&bundle)?;
    let localization = parse_localizations(&bundle)?;
    let mut findings = Vec::new();
    let mut canonical_items = Vec::new();

    for (path, kind, key_names, name_names) in [
        (
            "data/global/excel/weapons.txt",
            ItemKind::Base,
            &["code"][..],
            &["namestr", "name"][..],
        ),
        (
            "data/global/excel/armor.txt",
            ItemKind::Base,
            &["code"][..],
            &["namestr", "name"][..],
        ),
        (
            "data/global/excel/misc.txt",
            ItemKind::Base,
            &["code"][..],
            &["namestr", "name"][..],
        ),
        (
            "data/global/excel/uniqueitems.txt",
            ItemKind::Unique,
            &["index"][..],
            &["namestr", "index"][..],
        ),
        (
            "data/global/excel/setitems.txt",
            ItemKind::SetItem,
            &["index"][..],
            &["namestr", "index"][..],
        ),
        (
            "data/global/excel/runes.txt",
            ItemKind::Runeword,
            &["name", "index"][..],
            &["namestr", "rune name", "name"][..],
        ),
    ] {
        for row in parse_tsv(path, bundle.required(path)?)? {
            if row.get(&["enabled"]) == Some("0") || row.get(&["complete"]) == Some("0") {
                continue;
            }
            if matches!(kind, ItemKind::Unique | ItemKind::SetItem)
                && (row.get(&["disabled"]) == Some("1") || row.get(&["spawnable"]) == Some("0"))
            {
                continue;
            }
            if (kind == ItemKind::Unique && row.get(&["code"]).is_none())
                || (kind == ItemKind::SetItem && row.get(&["item", "code"]).is_none())
            {
                continue;
            }
            let Some(source_key) = row.get(key_names) else {
                continue;
            };
            let name_key = row.get(name_names).unwrap_or(source_key);
            let names = localized_names(name_key, &localization, &mut findings);
            canonical_items.push(CanonicalItem {
                id: CanonicalItemId {
                    kind,
                    source_key: source_key.to_owned(),
                },
                source_table: path.rsplit('/').next().unwrap_or(path).to_owned(),
                source_key: source_key.to_owned(),
                names,
            });
        }
    }

    let properties = parse_tsv(
        "data/global/excel/properties.txt",
        bundle.required("data/global/excel/properties.txt")?,
    )?;
    let property_map: BTreeMap<String, Row> = properties
        .into_iter()
        .filter_map(|row| {
            let code = row.get(&["code"])?.to_owned();
            Some((code, row))
        })
        .collect();
    let stats: BTreeSet<String> = parse_tsv(
        "data/global/excel/itemstatcost.txt",
        bundle.required("data/global/excel/itemstatcost.txt")?,
    )?
    .into_iter()
    .filter_map(|row| row.get(&["stat"]).map(str::to_owned))
    .collect();
    let skill_rows = parse_tsv(
        "data/global/excel/skills.txt",
        bundle.required("data/global/excel/skills.txt")?,
    )?;
    let skills = parse_skill_metadata(&skill_rows, &mut findings);
    let item_types: BTreeSet<String> = parse_tsv(
        "data/global/excel/itemtypes.txt",
        bundle.required("data/global/excel/itemtypes.txt")?,
    )?
    .into_iter()
    .filter_map(|row| row.get(&["code"]).map(str::to_owned))
    .collect();

    let mut affixes = Vec::new();
    for (path, table, kind) in [
        (
            "data/global/excel/magicprefix.txt",
            AffixTable::MagicPrefix,
            AffixKind::Prefix,
        ),
        (
            "data/global/excel/magicsuffix.txt",
            AffixTable::MagicSuffix,
            AffixKind::Suffix,
        ),
        (
            "data/global/excel/automagic.txt",
            AffixTable::AutoMagic,
            AffixKind::Automatic,
        ),
    ] {
        for row in parse_tsv(path, bundle.required(path)?)? {
            if row.get(&["spawnable"]) != Some("1") {
                continue;
            }
            let Some(name_key) = row.get(&["name", "namestr"]) else {
                continue;
            };
            let row_id = row.integer(&["id", "*id"], row.number)?;
            let id = CanonicalAffixId { table, row_id };
            let reference = id.to_string();
            let allowed_item_type_keys = numbered_cells(&row, "itype", 7);
            let excluded_item_type_keys = numbered_cells(&row, "etype", 5);
            record_unknown_item_types(
                allowed_item_type_keys
                    .iter()
                    .chain(excluded_item_type_keys.iter()),
                &item_types,
                &reference,
                &mut findings,
            );
            let mut modifiers = Vec::new();
            for slot in 1..=3 {
                let code_name = format!("mod{slot}code");
                let Some(property_code) = row.get(&[&code_name]) else {
                    continue;
                };
                let min = signed_integer(&row, &format!("mod{slot}min"), 0)?;
                let max = signed_integer(&row, &format!("mod{slot}max"), min)?;
                let parameter = optional_signed_integer(&row, &format!("mod{slot}param"))?;
                let source_operands = SourceOperands {
                    parameter,
                    min,
                    max,
                };
                match property_map.get(property_code) {
                    None => findings.push(AuditFinding {
                        severity: FindingSeverity::Error,
                        code: "unknown_property".to_owned(),
                        reference: reference.clone(),
                        message: format!("property {property_code:?} is not present in properties"),
                    }),
                    Some(property) => {
                        for stat in numbered_cells(property, "stat", 7) {
                            if !stats.contains(&stat) {
                                findings.push(AuditFinding {
                                    severity: FindingSeverity::Error,
                                    code: "unknown_stat".to_owned(),
                                    reference: reference.clone(),
                                    message: format!(
                                        "property {property_code:?} uses unknown stat {stat:?}"
                                    ),
                                });
                            }
                        }
                    }
                }
                let interpretation = interpret_modifier(
                    property_code,
                    &source_operands,
                    property_map.get(property_code),
                    &skills,
                    &reference,
                    &mut findings,
                );
                modifiers.push(AffixModifier {
                    property_code: property_code.to_owned(),
                    source_operands,
                    interpretation,
                });
            }
            affixes.push(AffixDefinition {
                id,
                affix_kind: kind,
                name_key: name_key.to_owned(),
                names: localized_names(name_key, &localization, &mut findings),
                level: row.integer(&["level"], 0)?,
                level_requirement: row.integer(&["levelreq", "level requirement"], 0)?,
                group: row.integer(&["group"], 0)?,
                frequency: row.integer(&["frequency"], 0)?,
                allowed_item_type_keys,
                excluded_item_type_keys,
                modifiers,
            });
        }
    }

    let aliases = match bundle.files.get(ALIASES) {
        Some(bytes) => {
            serde_json::from_slice::<Vec<AliasRecord>>(bytes).map_err(|source| Error::Json {
                path: ALIASES.to_owned(),
                source,
            })?
        }
        None => Vec::new(),
    };
    let known_items: BTreeSet<_> = canonical_items.iter().map(|item| item.id.clone()).collect();
    for alias in &aliases {
        if !known_items.contains(&alias.canonical_item_id) {
            findings.push(AuditFinding {
                severity: FindingSeverity::Error,
                code: "unknown_alias_item".to_owned(),
                reference: alias.canonical_item_id.to_string(),
                message: format!(
                    "alias {:?} references an unknown canonical item",
                    alias.text
                ),
            });
        }
        if !(0.0..=1.0).contains(&alias.confidence) {
            return Err(Error::Message(format!(
                "alias {:?} confidence must be between zero and one",
                alias.text
            )));
        }
    }

    let mut snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        build,
        canonical_items,
        affixes,
        aliases,
        findings,
    };
    snapshot.sort_stably();
    Ok(snapshot)
}

impl InputBundle {
    fn load(input: &Path) -> Result<Self> {
        if input.is_dir() {
            Self::from_directory(input)
        } else {
            Self::from_archive(input)
        }
    }

    fn from_directory(root: &Path) -> Result<Self> {
        let canonical_root = fs::canonicalize(root).map_err(|source| error::io(root, source))?;
        let mut files = BTreeMap::new();
        collect_directory(&canonical_root, &canonical_root, &mut files)?;
        let bundle = Self { files };
        bundle.validate_manifest()?;
        Ok(bundle)
    }

    fn from_archive(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| error::io(path, source))?;
        let mut archive = tar::Archive::new(file);
        let mut files = BTreeMap::new();
        let entries = archive
            .entries()
            .map_err(|source| error::io(path, source))?;
        for entry in entries {
            let entry = entry.map_err(|source| error::io(path, source))?;
            if !entry.header().entry_type().is_file() {
                return Err(Error::Message(
                    "archive may contain regular files only".to_owned(),
                ));
            }
            let entry_path = entry.path().map_err(|source| error::io(path, source))?;
            let name = safe_path_string(&entry_path)?;
            if entry.size() > MAX_INPUT_BYTES {
                return Err(Error::Message(format!(
                    "archive entry {name} exceeds 64 MiB"
                )));
            }
            let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            entry
                .take(MAX_INPUT_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|source| error::io(path, source))?;
            if files.insert(name.clone(), bytes).is_some() {
                return Err(Error::Message(format!("duplicate archive entry: {name}")));
            }
        }
        let bundle = Self { files };
        bundle.validate_manifest()?;
        Ok(bundle)
    }

    fn required(&self, path: &str) -> Result<&[u8]> {
        self.files
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| Error::MissingInput(path.to_owned()))
    }

    fn validate_manifest(&self) -> Result<()> {
        let expected: BTreeSet<&str> = SOURCE_WHITELIST
            .iter()
            .copied()
            .chain([BUILD_INFO, ALIASES])
            .collect();
        for required in SOURCE_WHITELIST.iter().copied().chain([BUILD_INFO]) {
            if !self.files.contains_key(required) {
                return Err(Error::MissingInput(required.to_owned()));
            }
        }
        for actual in self.files.keys() {
            if !expected.contains(actual.as_str()) {
                return Err(Error::Message(format!("unexpected input file: {actual}")));
            }
        }
        Ok(())
    }
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let entries = fs::read_dir(directory).map_err(|source| error::io(directory, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| error::io(directory, source))?;
        let file_type = entry
            .file_type()
            .map_err(|source| error::io(entry.path(), source))?;
        if file_type.is_symlink() {
            return Err(Error::UnsafePath(entry.path().display().to_string()));
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_directory(root, &path, files)?;
        } else if file_type.is_file() {
            let canonical = fs::canonicalize(&path).map_err(|source| error::io(&path, source))?;
            if !canonical.starts_with(root) {
                return Err(Error::UnsafePath(path.display().to_string()));
            }
            let relative = canonical
                .strip_prefix(root)
                .map_err(|_| Error::UnsafePath(path.display().to_string()))?;
            let name = safe_path_string(relative)?;
            let metadata =
                fs::metadata(&canonical).map_err(|source| error::io(&canonical, source))?;
            if metadata.len() > MAX_INPUT_BYTES {
                return Err(Error::Message(format!("input {name} exceeds 64 MiB")));
            }
            let bytes = fs::read(&canonical).map_err(|source| error::io(&canonical, source))?;
            files.insert(name, bytes);
        } else {
            return Err(Error::Message(format!(
                "unsupported input file type: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn safe_path_string(path: &Path) -> Result<String> {
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::UnsafePath(path.display().to_string()));
    }
    Ok(path
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn parse_build(bundle: &InputBundle) -> Result<GameBuild> {
    let rows = parse_build_info(bundle.required(BUILD_INFO)?)?;
    let row = rows
        .iter()
        .find(|row| row.get(&["active", "active!dec:1"]) != Some("0"))
        .ok_or_else(|| Error::Message(".build.info has no active row".to_owned()))?;
    let required_field = |names: &[&str], label: &str| {
        row.get(names)
            .map(str::to_owned)
            .ok_or_else(|| Error::Message(format!(".build.info lacks {label}")))
    };
    let mut input_sha256 = bundle
        .files
        .iter()
        .map(|(path, bytes)| InputDigest {
            path: path.clone(),
            sha256: hex_digest(bytes),
        })
        .collect::<Vec<_>>();
    input_sha256.sort();
    let product_names = &["product", "product!string:0"];
    let product = match row.get(product_names) {
        Some(product) => product.to_owned(),
        None if row.has_field(product_names) => "d2r".to_owned(),
        None => return Err(Error::Message(".build.info lacks Product".to_owned())),
    };
    Ok(GameBuild {
        product,
        build_key: required_field(&["build key", "build key!hex:16"], "Build Key")?,
        version: required_field(&["version", "version!string:0"], "Version")?,
        input_sha256,
    })
}

fn parse_localizations(bundle: &InputBundle) -> Result<Localization> {
    let mut values = BTreeMap::new();
    for path in [
        "data/local/lng/strings/item-names.json",
        "data/local/lng/strings/item-nameaffixes.json",
    ] {
        let value = parse_localization_json(path, bundle.required(path)?)?;
        let entries = value
            .as_array()
            .or_else(|| value.get("entries").and_then(Value::as_array))
            .ok_or_else(|| Error::Message(format!("{path} must be an array or entries object")))?;
        for entry in entries {
            let object = entry
                .as_object()
                .ok_or_else(|| Error::Message(format!("{path} contains a non-object entry")))?;
            let key = ["Key", "key", "id"]
                .iter()
                .find_map(|name| object.get(*name).and_then(Value::as_str))
                .ok_or_else(|| Error::Message(format!("{path} entry lacks an internal key")))?;
            let target = values.entry(key.to_owned()).or_insert_with(BTreeMap::new);
            for (field, locale) in [
                ("enUS", Locale::EnUs),
                ("zhTW", Locale::ZhTw),
                ("zhCN", Locale::ZhCn),
            ] {
                if let Some(text) = object
                    .get(field)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    target.insert(locale, text.to_owned());
                }
            }
        }
    }
    Ok(Localization { values })
}

fn parse_localization_json(path: &str, bytes: &[u8]) -> Result<Value> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    serde_json::from_slice(bytes).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })
}

fn parse_skill_metadata(
    rows: &[Row],
    findings: &mut Vec<AuditFinding>,
) -> BTreeMap<u32, SkillMetadata> {
    let mut skills: BTreeMap<u32, SkillMetadata> = BTreeMap::new();
    let mut conflicting_ids = BTreeSet::new();
    for row in rows {
        let reference = format!("skills:{}", row.number);
        let Some(raw_id) = row.get(&["id", "*id"]) else {
            findings.push(AuditFinding {
                severity: FindingSeverity::Error,
                code: "invalid_skill_id".to_owned(),
                reference,
                message: "skill row has no numeric ID".to_owned(),
            });
            continue;
        };
        let Ok(skill_id) = raw_id.parse::<u32>() else {
            findings.push(AuditFinding {
                severity: FindingSeverity::Error,
                code: "invalid_skill_id".to_owned(),
                reference,
                message: format!("skill row has invalid ID {raw_id:?}"),
            });
            continue;
        };
        let required_level = match row.get(&["reqlevel"]) {
            Some(raw_level) => match raw_level.parse::<u32>() {
                Ok(level) => level,
                Err(_) => {
                    findings.push(AuditFinding {
                        severity: FindingSeverity::Error,
                        code: "invalid_skill_required_level".to_owned(),
                        reference,
                        message: format!(
                            "skill {skill_id} has invalid required level {raw_level:?}"
                        ),
                    });
                    conflicting_ids.insert(skill_id);
                    skills.remove(&skill_id);
                    continue;
                }
            },
            None => 0,
        };
        let metadata = SkillMetadata { required_level };
        if conflicting_ids.contains(&skill_id) {
            continue;
        }
        match skills.get(&skill_id) {
            Some(previous) if *previous != metadata => {
                findings.push(AuditFinding {
                    severity: FindingSeverity::Error,
                    code: "conflicting_skill_required_level".to_owned(),
                    reference,
                    message: format!(
                        "skill {skill_id} has conflicting required levels {} and {required_level}",
                        previous.required_level
                    ),
                });
                conflicting_ids.insert(skill_id);
                skills.remove(&skill_id);
            }
            Some(_) => {}
            None => {
                skills.insert(skill_id, metadata);
            }
        }
    }
    skills
}

fn record_unknown_item_types<'a>(
    values: impl Iterator<Item = &'a String>,
    known: &BTreeSet<String>,
    reference: &str,
    findings: &mut Vec<AuditFinding>,
) {
    for item_type in values {
        if !known.contains(item_type) {
            findings.push(AuditFinding {
                severity: FindingSeverity::Gap,
                code: "unknown_item_type".to_owned(),
                reference: reference.to_owned(),
                message: format!("item type {item_type:?} is not present in itemtypes"),
            });
        }
    }
}

fn interpret_modifier(
    property_code: &str,
    operands: &SourceOperands,
    property: Option<&Row>,
    skills: &BTreeMap<u32, SkillMetadata>,
    reference: &str,
    findings: &mut Vec<AuditFinding>,
) -> ModifierInterpretation {
    let Some(property) = property else {
        return ModifierInterpretation::Unknown {
            ui_range_type: None,
        };
    };
    let range_type = match property.get(&["uiRangeType"]) {
        None => 5,
        Some(value) => match value.parse::<u32>() {
            Ok(value) => value,
            Err(_) => {
                findings.push(AuditFinding {
                    severity: FindingSeverity::Error,
                    code: "invalid_ui_range_type".to_owned(),
                    reference: reference.to_owned(),
                    message: format!(
                        "property {property_code:?} has invalid uiRangeType {value:?}"
                    ),
                });
                return ModifierInterpretation::Unknown {
                    ui_range_type: None,
                };
            }
        },
    };
    match range_type {
        5 => ModifierInterpretation::NumericRange {
            minimum: operands.min.min(operands.max),
            maximum: operands.min.max(operands.max),
        },
        6 if property.get(&["func1"]) == Some("19") => interpret_skill_modifier(
            property_code,
            operands,
            range_type,
            skills,
            reference,
            findings,
        ),
        6 => {
            findings.push(AuditFinding {
                severity: FindingSeverity::Gap,
                code: "uninterpreted_modifier".to_owned(),
                reference: reference.to_owned(),
                message: format!(
                    "property {property_code:?} has uiRangeType 6 without function 19"
                ),
            });
            ModifierInterpretation::Unknown {
                ui_range_type: Some(6),
            }
        }
        7 => interpret_skill_modifier(
            property_code,
            operands,
            range_type,
            skills,
            reference,
            findings,
        ),
        ui_range_type => {
            findings.push(AuditFinding {
                severity: FindingSeverity::Gap,
                code: "uninterpreted_modifier".to_owned(),
                reference: reference.to_owned(),
                message: format!(
                    "property {property_code:?} has unsupported uiRangeType {ui_range_type}"
                ),
            });
            ModifierInterpretation::Unknown {
                ui_range_type: Some(ui_range_type),
            }
        }
    }
}

fn interpret_skill_modifier(
    property_code: &str,
    operands: &SourceOperands,
    range_type: u32,
    skills: &BTreeMap<u32, SkillMetadata>,
    reference: &str,
    findings: &mut Vec<AuditFinding>,
) -> ModifierInterpretation {
    let skill_id = operands
        .parameter
        .and_then(|value| u32::try_from(value).ok());
    let skill = skill_id.and_then(|skill_id| skills.get(&skill_id));
    if skill.is_none() {
        let (code, detail) = if operands.parameter.is_none() {
            (
                "missing_skill_parameter",
                "has no skill parameter".to_owned(),
            )
        } else {
            (
                "unknown_skill",
                format!("references unknown skill {:?}", operands.parameter),
            )
        };
        findings.push(AuditFinding {
            severity: FindingSeverity::Error,
            code: code.to_owned(),
            reference: reference.to_owned(),
            message: format!("property {property_code:?} {detail}"),
        });
        return ModifierInterpretation::Unknown {
            ui_range_type: Some(range_type),
        };
    }
    let skill_id = skill_id.expect("checked above");
    let skill = skill.expect("checked above");
    if range_type == 7 {
        if operands.min < 0 || operands.max < 0 {
            findings.push(AuditFinding {
                severity: FindingSeverity::Error,
                code: "invalid_skill_operands".to_owned(),
                reference: reference.to_owned(),
                message: format!("property {property_code:?} has negative chance or skill level"),
            });
            return ModifierInterpretation::Unknown {
                ui_range_type: Some(range_type),
            };
        }
        ModifierInterpretation::ChanceToCast {
            skill_id: i32::try_from(skill_id).expect("source parameter was an i32"),
            chance_percent: if operands.min == 0 { 5 } else { operands.min },
            skill_level: operands.max,
        }
    } else if operands.min < 0 && operands.max < 0 {
        let raw_step = (99_i64 - i64::from(skill.required_level)) / i64::from(operands.max).abs();
        let item_levels_per_skill_level = u32::try_from(raw_step.max(1))
            .expect("item-level step is bounded by the engine level cap");
        let base_charges =
            u32::try_from(i64::from(operands.min).abs()).expect("absolute i32 operand fits u32");
        ModifierInterpretation::ScaledChargedSkill(ScaledChargedSkill {
            skill_id,
            skill_required_level: skill.required_level,
            item_levels_per_skill_level,
            base_charges,
        })
    } else if operands.min >= 0 && operands.max > 0 {
        ModifierInterpretation::ChargedSkill {
            skill_id: i32::try_from(skill_id).expect("source parameter was an i32"),
            max_charges: if operands.min == 0 {
                5
            } else {
                operands.min.clamp(1, 255)
            },
            skill_level: operands.max,
        }
    } else {
        findings.push(AuditFinding {
            severity: FindingSeverity::Gap,
            code: "uninterpreted_skill_operands".to_owned(),
            reference: reference.to_owned(),
            message: format!(
                "property {property_code:?} has unsupported charged-skill operand signs"
            ),
        });
        ModifierInterpretation::Unknown {
            ui_range_type: Some(range_type),
        }
    }
}

fn localized_names(
    key: &str,
    localization: &Localization,
    findings: &mut Vec<AuditFinding>,
) -> Vec<LocalizedName> {
    let mut result = Vec::new();
    let values = localization.values.get(key);
    for locale in [Locale::EnUs, Locale::ZhTw, Locale::ZhCn] {
        match values.and_then(|entries| entries.get(&locale)) {
            Some(text) => result.push(LocalizedName {
                locale,
                string_key: key.to_owned(),
                text: text.clone(),
            }),
            None => findings.push(AuditFinding {
                severity: FindingSeverity::Gap,
                code: "missing_locale".to_owned(),
                reference: key.to_owned(),
                message: format!("localization key {key:?} lacks {locale:?}"),
            }),
        }
    }
    result
}

fn parse_tsv(path: &str, bytes: &[u8]) -> Result<Vec<Row>> {
    parse_delimited(path, bytes, b'\t')
}

fn parse_build_info(bytes: &[u8]) -> Result<Vec<Row>> {
    let header = bytes.split(|byte| *byte == b'\n').next().unwrap_or(bytes);
    let delimiter = if header.contains(&b'|') { b'|' } else { b'\t' };
    parse_delimited(BUILD_INFO, bytes, delimiter)
}

fn parse_delimited(path: &str, bytes: &[u8], delimiter: u8) -> Result<Vec<Row>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(Cursor::new(bytes));
    let headers = reader
        .headers()
        .map_err(|source| Error::Tsv {
            path: path.to_owned(),
            source,
        })?
        .iter()
        .map(normalize_header)
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|source| Error::Tsv {
            path: path.to_owned(),
            source,
        })?;
        if record.iter().all(|field| field.trim().is_empty()) {
            continue;
        }
        rows.push(Row {
            number: u32::try_from(index + 1)
                .map_err(|_| Error::Message(format!("{path} has too many rows")))?,
            fields: zip_record(&headers, &record),
        });
    }
    Ok(rows)
}

fn normalize_header(header: &str) -> String {
    header
        .trim_start_matches('\u{feff}')
        .trim()
        .to_ascii_lowercase()
}

fn zip_record(headers: &[String], record: &StringRecord) -> BTreeMap<String, String> {
    headers
        .iter()
        .zip(record.iter())
        .map(|(header, value)| (header.clone(), value.trim().to_owned()))
        .collect()
}

fn numbered_cells(row: &Row, prefix: &str, maximum: usize) -> Vec<String> {
    (1..=maximum)
        .filter_map(|index| row.get(&[&format!("{prefix}{index}")]).map(str::to_owned))
        .collect()
}

fn optional_signed_integer(row: &Row, name: &str) -> Result<Option<i32>> {
    row.get(&[name])
        .map(|value| {
            value.parse().map_err(|_| {
                Error::Message(format!(
                    "row {} has invalid integer {value:?} in {name}",
                    row.number
                ))
            })
        })
        .transpose()
}

fn signed_integer(row: &Row, name: &str, default: i32) -> Result<i32> {
    Ok(optional_signed_integer(row, name)?.unwrap_or(default))
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn atomic_write(output: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| error::io(parent, source))?;
    let file_name = output
        .file_name()
        .ok_or_else(|| Error::Message("output path must name a file".to_owned()))?
        .to_string_lossy();
    let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| error::io(&temporary, source))?;
        file.write_all(bytes)
            .map_err(|source| error::io(&temporary, source))?;
        file.sync_all()
            .map_err(|source| error::io(&temporary, source))?;
        fs::rename(&temporary, output).map_err(|source| error::io(output, source))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(range_type: Option<&str>) -> Row {
        let mut fields = BTreeMap::new();
        if let Some(range_type) = range_type {
            fields.insert("uirangetype".to_owned(), range_type.to_owned());
            if range_type == "6" {
                fields.insert("func1".to_owned(), "19".to_owned());
            }
        }
        Row { number: 1, fields }
    }

    fn operands(parameter: Option<i32>, min: i32, max: i32) -> SourceOperands {
        SourceOperands {
            parameter,
            min,
            max,
        }
    }

    fn interpret(
        range_type: Option<&str>,
        operands: SourceOperands,
        skills: &[&str],
    ) -> (ModifierInterpretation, Vec<AuditFinding>) {
        let skills = skills
            .iter()
            .map(|value| (value.parse().unwrap(), 0))
            .collect::<Vec<_>>();
        interpret_with_metadata(range_type, operands, &skills)
    }

    fn interpret_with_metadata(
        range_type: Option<&str>,
        operands: SourceOperands,
        skills: &[(u32, u32)],
    ) -> (ModifierInterpretation, Vec<AuditFinding>) {
        let property = property(range_type);
        let skills = skills
            .iter()
            .map(|(id, required_level)| {
                (
                    *id,
                    SkillMetadata {
                        required_level: *required_level,
                    },
                )
            })
            .collect();
        let mut findings = Vec::new();
        let interpretation = interpret_modifier(
            "fixture",
            &operands,
            Some(&property),
            &skills,
            "fixture:1",
            &mut findings,
        );
        (interpretation, findings)
    }

    fn build_info(contents: &[u8]) -> InputBundle {
        InputBundle {
            files: BTreeMap::from([(BUILD_INFO.to_owned(), contents.to_vec())]),
        }
    }

    #[test]
    fn nonempty_build_product_is_preserved() {
        let bundle =
            build_info(b"Active\tBuild Key\tVersion\tProduct\n1\tbuild\tversion\tcustom\n");

        assert_eq!(parse_build(&bundle).unwrap().product, "custom");
    }

    #[test]
    fn empty_typed_build_product_defaults_to_d2r() {
        let bundle = build_info(
            b"Active!DEC:1|Product!STRING:0|Build Key!HEX:16|Version!STRING:0\n1||build|version\n",
        );

        assert_eq!(parse_build(&bundle).unwrap().product, "d2r");
    }

    #[test]
    fn missing_build_product_is_rejected() {
        let bundle = build_info(b"Active|Build Key|Version\n1|build|version\n");

        assert_eq!(
            parse_build(&bundle).unwrap_err().to_string(),
            ".build.info lacks Product"
        );
    }

    #[test]
    fn generic_tsv_parser_does_not_detect_pipe_delimiters() {
        let rows = parse_tsv("fixture", b"id|value\n1|kept\n").expect("valid TSV");

        assert_eq!(rows[0].get(&["id"]), None);
        assert_eq!(rows[0].get(&["id|value"]), Some("1|kept"));
    }

    #[test]
    fn empty_cells_stay_absent_and_ranges_are_checked() {
        let rows = parse_tsv("fixture", b"id\tvalue\n1\t\n").expect("valid TSV");
        assert_eq!(rows[0].get(&["value"]), None);
        assert_eq!(optional_signed_integer(&rows[0], "value").unwrap(), None);
    }

    #[test]
    fn skill_metadata_accepts_id_headers_and_required_levels() {
        for header in ["*Id", "Id", "id"] {
            let input = format!("{header}\treqlevel\n1001\t\n1002\t24\n");
            let rows = parse_tsv("skills", input.as_bytes()).unwrap();
            let mut findings = Vec::new();
            let skills = parse_skill_metadata(&rows, &mut findings);

            assert_eq!(skills[&1001].required_level, 0);
            assert_eq!(skills[&1002].required_level, 24);
            assert!(findings.is_empty());
        }
    }

    #[test]
    fn malformed_and_conflicting_skill_metadata_is_not_selected() {
        let rows = parse_tsv(
            "skills",
            b"id\treqlevel\ninvalid\t1\n1001\tbad\n1002\t2\n1002\t3\n1003\t-1\n",
        )
        .unwrap();
        let mut findings = Vec::new();
        let skills = parse_skill_metadata(&rows, &mut findings);

        assert!(skills.is_empty());
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.code.as_str())
                .collect::<Vec<_>>(),
            [
                "invalid_skill_id",
                "invalid_skill_required_level",
                "conflicting_skill_required_level",
                "invalid_skill_required_level",
            ]
        );
    }

    #[test]
    fn unknown_item_types_remain_explicit_gaps() {
        let values = ["missing_fixture_type".to_owned()];
        let mut findings = Vec::new();
        record_unknown_item_types(values.iter(), &BTreeSet::new(), "fixture:1", &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Gap);
        assert_eq!(findings[0].code, "unknown_item_type");
    }

    #[test]
    fn localization_json_accepts_exactly_one_leading_bom() {
        let plain = br#"[{"Key":"fixture"}]"#;
        let with_bom = [b"\xef\xbb\xbf".as_slice(), plain].concat();

        assert_eq!(
            parse_localization_json("fixture", plain).unwrap(),
            parse_localization_json("fixture", &with_bom).unwrap()
        );
        assert!(parse_localization_json("fixture", b"not json").is_err());
        assert!(parse_localization_json("fixture", b"\xff").is_err());
    }

    #[test]
    fn numeric_ranges_order_only_the_interpretation() {
        let source = operands(None, 10, 3);
        let (interpretation, findings) = interpret(None, source.clone(), &[]);

        assert_eq!(source, operands(None, 10, 3));
        assert_eq!(
            interpretation,
            ModifierInterpretation::NumericRange {
                minimum: 3,
                maximum: 10
            }
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn chance_and_charged_operands_have_distinct_meanings() {
        let (chance, chance_findings) = interpret(Some("7"), operands(Some(1001), 0, 8), &["1001"]);
        let (charged, charged_findings) =
            interpret(Some("6"), operands(Some(1002), 12, 6), &["1002"]);

        assert_eq!(
            chance,
            ModifierInterpretation::ChanceToCast {
                skill_id: 1001,
                chance_percent: 5,
                skill_level: 8
            }
        );
        assert_eq!(
            charged,
            ModifierInterpretation::ChargedSkill {
                skill_id: 1002,
                max_charges: 12,
                skill_level: 6
            }
        );
        assert!(chance_findings.is_empty());
        assert!(charged_findings.is_empty());
    }

    #[test]
    fn fixed_charged_defaults_and_caps_maximum_charges() {
        let (defaulted, default_findings) =
            interpret(Some("6"), operands(Some(1002), 0, 6), &["1002"]);
        let (capped, capped_findings) =
            interpret(Some("6"), operands(Some(1002), 999, 6), &["1002"]);

        assert_eq!(
            defaulted,
            ModifierInterpretation::ChargedSkill {
                skill_id: 1002,
                max_charges: 5,
                skill_level: 6,
            }
        );
        assert_eq!(
            capped,
            ModifierInterpretation::ChargedSkill {
                skill_id: 1002,
                max_charges: 255,
                skill_level: 6,
            }
        );
        assert!(default_findings.is_empty());
        assert!(capped_findings.is_empty());
    }

    #[test]
    fn negative_charged_operands_become_lossless_scaled_inputs() {
        let source = operands(Some(900_003), -40, -15);
        let (interpretation, findings) =
            interpret_with_metadata(Some("6"), source.clone(), &[(900_003, 24)]);

        assert_eq!(source, operands(Some(900_003), -40, -15));
        assert_eq!(
            interpretation,
            ModifierInterpretation::ScaledChargedSkill(ScaledChargedSkill {
                skill_id: 900_003,
                skill_required_level: 24,
                item_levels_per_skill_level: 5,
                base_charges: 40,
            })
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn scaled_step_uses_signed_truncation_and_a_minimum_of_one() {
        for required_level in [100, 110] {
            let (interpretation, findings) = interpret_with_metadata(
                Some("6"),
                operands(Some(900_003), -8, -7),
                &[(900_003, required_level)],
            );
            let ModifierInterpretation::ScaledChargedSkill(effect) = interpretation else {
                panic!("expected scaled charged skill");
            };
            assert_eq!(effect.item_levels_per_skill_level, 1);
            assert!(findings.is_empty());
        }
    }

    #[test]
    fn unsupported_charged_shapes_are_lossless_gaps() {
        for (min, max) in [(10, 0), (-10, 2), (10, -2)] {
            let source = operands(Some(1002), min, max);
            let (interpretation, findings) = interpret(Some("6"), source.clone(), &["1002"]);

            assert_eq!(source, operands(Some(1002), min, max));
            assert_eq!(
                interpretation,
                ModifierInterpretation::Unknown {
                    ui_range_type: Some(6)
                }
            );
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, FindingSeverity::Gap);
            assert_eq!(findings[0].code, "uninterpreted_skill_operands");
        }
    }

    #[test]
    fn negative_chance_operands_remain_errors() {
        let (_, findings) = interpret(Some("7"), operands(Some(1001), -1, 8), &["1001"]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert_eq!(findings[0].code, "invalid_skill_operands");
    }

    #[test]
    fn unsupported_and_invalid_range_types_are_lossless_unknowns() {
        let (unsupported, unsupported_findings) =
            interpret(Some("3"), operands(Some(9), 10, 2), &[]);
        let (invalid, invalid_findings) = interpret(Some("invalid"), operands(Some(9), 10, 2), &[]);

        assert_eq!(
            unsupported,
            ModifierInterpretation::Unknown {
                ui_range_type: Some(3)
            }
        );
        assert_eq!(unsupported_findings[0].severity, FindingSeverity::Gap);
        assert_eq!(
            invalid,
            ModifierInterpretation::Unknown {
                ui_range_type: None
            }
        );
        assert_eq!(invalid_findings[0].severity, FindingSeverity::Error);
    }

    #[test]
    fn known_skill_types_require_a_present_known_skill() {
        let (missing, missing_findings) = interpret(Some("7"), operands(None, 5, 2), &[]);
        let (unknown, unknown_findings) =
            interpret(Some("6"), operands(Some(999), 12, 2), &["1001"]);

        assert!(matches!(missing, ModifierInterpretation::Unknown { .. }));
        assert_eq!(missing_findings[0].code, "missing_skill_parameter");
        assert!(matches!(unknown, ModifierInterpretation::Unknown { .. }));
        assert_eq!(unknown_findings[0].code, "unknown_skill");
    }

    #[test]
    fn minimal_fixture_models_real_affix_activity_and_reference_boundaries() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/d2r-minimal");
        let snapshot = normalize_input(&fixture).expect("fixture normalizes");

        let item_ids = snapshot
            .canonical_items
            .iter()
            .map(|item| item.id.to_string())
            .collect::<BTreeSet<_>>();
        for expected in [
            "unique:Ars Al'Diablolos",
            "unique:Modern Valid Unique",
            "unique:Blank Spawnable Unique",
            "set-item:Modern Valid Set",
            "set-item:Legacy Code Set",
        ] {
            assert!(
                item_ids.contains(expected),
                "missing admitted fixture {expected}"
            );
        }
        for rejected in [
            "unique:Legacy Disabled Unique",
            "unique:Legacy Incomplete Unique",
            "unique:Modern Disabled Unique",
            "unique:Unspawnable Unique",
            "unique:Rings",
            "set-item:Disabled Set",
            "set-item:Unspawnable Set",
            "set-item:Expansion",
        ] {
            assert!(
                !item_ids.contains(rejected),
                "admitted rejected fixture {rejected}"
            );
        }

        let affix = |table, row_id| {
            snapshot
                .affixes
                .iter()
                .find(|affix| affix.id == CanonicalAffixId { table, row_id })
        };

        assert!(affix(AffixTable::MagicSuffix, 202).is_none());
        assert!(affix(AffixTable::MagicSuffix, 203).is_none());

        let cost_only = affix(AffixTable::MagicSuffix, 204).expect("active cost-only affix");
        assert!(cost_only.modifiers.is_empty());
        assert_eq!(cost_only.allowed_item_type_keys, ["book"]);

        let chance = &affix(AffixTable::MagicSuffix, 201)
            .expect("chance affix")
            .modifiers[0];
        assert_eq!(chance.source_operands, operands(Some(1001), 0, 8));
        assert_eq!(
            chance.interpretation,
            ModifierInterpretation::ChanceToCast {
                skill_id: 1001,
                chance_percent: 5,
                skill_level: 8,
            }
        );

        let charged = &affix(AffixTable::AutoMagic, 301)
            .expect("charged affix")
            .modifiers[0];
        assert_eq!(charged.source_operands, operands(Some(1002), 12, 6));
        assert_eq!(
            charged.interpretation,
            ModifierInterpretation::ChargedSkill {
                skill_id: 1002,
                max_charges: 12,
                skill_level: 6,
            }
        );

        let scaled = &affix(AffixTable::MagicSuffix, 205)
            .expect("scaled charged affix")
            .modifiers[0];
        assert_eq!(scaled.source_operands, operands(Some(900_003), -40, -15));
        assert_eq!(
            scaled.interpretation,
            ModifierInterpretation::ScaledChargedSkill(ScaledChargedSkill {
                skill_id: 900_003,
                skill_required_level: 24,
                item_levels_per_skill_level: 5,
                base_charges: 40,
            })
        );

        let numeric = &affix(AffixTable::MagicPrefix, 101)
            .expect("numeric affix")
            .modifiers[0];
        assert_eq!(numeric.source_operands, operands(None, 4, 1));
        assert_eq!(
            numeric.interpretation,
            ModifierInterpretation::NumericRange {
                minimum: 1,
                maximum: 4,
            }
        );

        let report = crate::audit_snapshot(&snapshot);
        assert!(report.passed);
        assert_eq!(report.error_count, 0);
        assert_eq!(report.gap_count, 0);
        assert!(report.warlock_sentinels.values().all(|value| *value));
    }

    #[test]
    fn modifier_serialization_is_deterministic_and_tagged() {
        let modifier = AffixModifier {
            property_code: "fixture".to_owned(),
            source_operands: operands(Some(1001), 0, 8),
            interpretation: ModifierInterpretation::ChanceToCast {
                skill_id: 1001,
                chance_percent: 5,
                skill_level: 8,
            },
        };

        let first = serde_json::to_vec(&modifier).unwrap();
        let second = serde_json::to_vec(&modifier).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            serde_json::from_slice::<Value>(&first).unwrap()["interpretation"]["kind"],
            "chance_to_cast"
        );
    }

    #[test]
    fn unsafe_archive_names_are_rejected() {
        assert!(safe_path_string(Path::new("../outside")).is_err());
        assert!(safe_path_string(Path::new("/absolute")).is_err());
        assert_eq!(
            safe_path_string(Path::new("safe/name")).unwrap(),
            "safe/name"
        );
    }
}
