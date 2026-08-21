use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameBuild {
    pub product: String,
    pub build_key: String,
    pub version: String,
    pub input_sha256: Vec<InputDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InputDigest {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ItemKind {
    Base,
    Unique,
    SetItem,
    Runeword,
}

impl ItemKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Unique => "unique",
            Self::SetItem => "set-item",
            Self::Runeword => "runeword",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalItemId {
    pub kind: ItemKind,
    pub source_key: String,
}

impl fmt::Display for CanonicalItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind.wire(), self.source_key)
    }
}

impl FromStr for CanonicalItemId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (kind, source_key) = value
            .split_once(':')
            .ok_or_else(|| "canonical item ID must contain ':'".to_owned())?;
        if source_key.is_empty() {
            return Err("canonical item source key cannot be empty".to_owned());
        }
        let kind = match kind {
            "base" => ItemKind::Base,
            "unique" => ItemKind::Unique,
            "set-item" => ItemKind::SetItem,
            "runeword" => ItemKind::Runeword,
            _ => return Err(format!("unknown canonical item kind: {kind}")),
        };
        Ok(Self {
            kind,
            source_key: source_key.to_owned(),
        })
    }
}

impl Serialize for CanonicalItemId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CanonicalItemId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AffixTable {
    #[serde(rename = "magicprefix")]
    MagicPrefix,
    #[serde(rename = "magicsuffix")]
    MagicSuffix,
    #[serde(rename = "automagic")]
    AutoMagic,
}

impl AffixTable {
    pub fn wire(self) -> &'static str {
        match self {
            Self::MagicPrefix => "magicprefix",
            Self::MagicSuffix => "magicsuffix",
            Self::AutoMagic => "automagic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalAffixId {
    pub table: AffixTable,
    pub row_id: u32,
}

impl fmt::Display for CanonicalAffixId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.table.wire(), self.row_id)
    }
}

impl FromStr for CanonicalAffixId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (table, row_id) = value
            .split_once(':')
            .ok_or_else(|| "canonical affix ID must contain ':'".to_owned())?;
        let table = match table {
            "magicprefix" => AffixTable::MagicPrefix,
            "magicsuffix" => AffixTable::MagicSuffix,
            "automagic" => AffixTable::AutoMagic,
            _ => return Err(format!("unknown affix table: {table}")),
        };
        Ok(Self {
            table,
            row_id: row_id
                .parse()
                .map_err(|_| format!("invalid affix row ID: {row_id}"))?,
        })
    }
}

impl Serialize for CanonicalAffixId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CanonicalAffixId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Locale {
    #[serde(rename = "enUS")]
    EnUs,
    #[serde(rename = "zhTW")]
    ZhTw,
    #[serde(rename = "zhCN")]
    ZhCn,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LocalizedName {
    pub locale: Locale,
    pub string_key: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CanonicalItem {
    pub id: CanonicalItemId,
    pub source_table: String,
    pub source_key: String,
    pub names: Vec<LocalizedName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasKind {
    LegacyTranslation,
    CommunityAbbreviation,
    MarketSpelling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    AuthoredFixture,
    Reviewed,
    Unreviewed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasRecord {
    pub canonical_item_id: CanonicalItemId,
    pub text: String,
    pub alias_kind: AliasKind,
    pub source: String,
    pub source_locator: String,
    pub observed_at: String,
    pub confidence: f64,
    pub review_status: ReviewStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AffixModifier {
    pub property_code: String,
    pub source_operands: SourceOperands,
    pub interpretation: ModifierInterpretation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOperands {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<i32>,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaledChargedSkill {
    pub skill_id: u32,
    pub skill_required_level: u32,
    pub item_levels_per_skill_level: u32,
    pub base_charges: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargedSkillValues {
    pub skill_level: u64,
    pub max_charges: u32,
}

impl ScaledChargedSkill {
    pub fn evaluate(&self, item_level: u64) -> ChargedSkillValues {
        let levels_above_requirement =
            item_level.saturating_sub(u64::from(self.skill_required_level));
        let skill_level =
            (levels_above_requirement / u64::from(self.item_levels_per_skill_level.max(1))).max(1);
        let scaled_charges = u128::from(self.base_charges)
            + u128::from(skill_level) * u128::from(self.base_charges) / 8;
        let max_charges = scaled_charges.clamp(1, 255) as u32;

        ChargedSkillValues {
            skill_level,
            max_charges,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModifierInterpretation {
    NumericRange {
        minimum: i32,
        maximum: i32,
    },
    ChanceToCast {
        skill_id: i32,
        chance_percent: i32,
        skill_level: i32,
    },
    ChargedSkill {
        skill_id: i32,
        max_charges: i32,
        skill_level: i32,
    },
    ScaledChargedSkill(ScaledChargedSkill),
    Unknown {
        #[serde(skip_serializing_if = "Option::is_none")]
        ui_range_type: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffixKind {
    Prefix,
    Suffix,
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AffixDefinition {
    pub id: CanonicalAffixId,
    pub affix_kind: AffixKind,
    pub name_key: String,
    pub names: Vec<LocalizedName>,
    pub level: u32,
    pub level_requirement: u32,
    pub group: u32,
    pub frequency: u32,
    pub allowed_item_type_keys: Vec<String>,
    pub excluded_item_type_keys: Vec<String>,
    pub modifiers: Vec<AffixModifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Gap,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFinding {
    pub severity: FindingSeverity,
    pub code: String,
    pub reference: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub build: GameBuild,
    pub canonical_items: Vec<CanonicalItem>,
    pub affixes: Vec<AffixDefinition>,
    pub aliases: Vec<AliasRecord>,
    pub findings: Vec<AuditFinding>,
}

impl Snapshot {
    pub fn sort_stably(&mut self) {
        self.build.input_sha256.sort();
        for item in &mut self.canonical_items {
            item.names.sort();
        }
        self.canonical_items.sort();
        self.canonical_items.dedup();
        self.affixes.sort_by(|a, b| a.id.cmp(&b.id));
        for affix in &mut self.affixes {
            affix.names.sort();
            affix.allowed_item_type_keys.sort();
            affix.allowed_item_type_keys.dedup();
            affix.excluded_item_type_keys.sort();
            affix.excluded_item_type_keys.dedup();
        }
        self.aliases.sort_by(|a, b| {
            (
                &a.canonical_item_id,
                &a.text,
                a.alias_kind,
                &a.source_locator,
            )
                .cmp(&(
                    &b.canonical_item_id,
                    &b.text,
                    b.alias_kind,
                    &b.source_locator,
                ))
        });
        self.findings.sort_by(|a, b| {
            (&a.severity, &a.code, &a.reference, &a.message).cmp(&(
                &b.severity,
                &b.code,
                &b.reference,
                &b.message,
            ))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(source_table: &str, source_key: &str, names: Vec<LocalizedName>) -> CanonicalItem {
        CanonicalItem {
            id: CanonicalItemId {
                kind: ItemKind::Unique,
                source_key: "synthetic-item".to_owned(),
            },
            source_table: source_table.to_owned(),
            source_key: source_key.to_owned(),
            names,
        }
    }

    fn snapshot(items: Vec<CanonicalItem>) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            build: GameBuild {
                product: "fixture".to_owned(),
                build_key: "fixture".to_owned(),
                version: "fixture".to_owned(),
                input_sha256: Vec::new(),
            },
            canonical_items: items,
            affixes: Vec::new(),
            aliases: Vec::new(),
            findings: Vec::new(),
        }
    }

    #[test]
    fn scaled_charges_evaluate_boundaries_and_cap_without_overflow() {
        let effect = ScaledChargedSkill {
            skill_id: 900_001,
            skill_required_level: 24,
            item_levels_per_skill_level: 5,
            base_charges: 20,
        };

        assert_eq!(
            effect.evaluate(0),
            ChargedSkillValues {
                skill_level: 1,
                max_charges: 22,
            }
        );
        assert_eq!(effect.evaluate(24), effect.evaluate(28));
        assert_eq!(effect.evaluate(29).skill_level, 1);
        assert_eq!(effect.evaluate(34).skill_level, 2);
        assert_eq!(effect.evaluate(34).max_charges, 25);

        let capped = ScaledChargedSkill {
            base_charges: 200,
            ..effect
        };
        assert_eq!(capped.evaluate(34).max_charges, 250);
        assert_eq!(capped.evaluate(39).max_charges, 255);
        assert_eq!(capped.evaluate(u64::MAX).max_charges, 255);
    }

    #[test]
    fn exact_items_deduplicate_after_nested_name_sorting() {
        let en = LocalizedName {
            locale: Locale::EnUs,
            string_key: "synthetic-name".to_owned(),
            text: "Synthetic Name".to_owned(),
        };
        let zh = LocalizedName {
            locale: Locale::ZhCn,
            string_key: "synthetic-name".to_owned(),
            text: "Synthetic Name CN".to_owned(),
        };
        let first = item(
            "uniqueitems.txt",
            "synthetic-item",
            vec![en.clone(), zh.clone()],
        );
        let second = item("uniqueitems.txt", "synthetic-item", vec![zh, en]);

        let mut forward = snapshot(vec![first.clone(), second.clone()]);
        let mut reverse = snapshot(vec![second, first]);
        forward.sort_stably();
        reverse.sort_stably();

        assert_eq!(forward, reverse);
        assert_eq!(forward.canonical_items.len(), 1);
    }

    #[test]
    fn unequal_items_with_the_same_identity_remain_distinct() {
        let base = item("uniqueitems.txt", "synthetic-item", Vec::new());
        for different in [
            item("setitems.txt", "synthetic-item", Vec::new()),
            item("uniqueitems.txt", "different-source-key", Vec::new()),
            item(
                "uniqueitems.txt",
                "synthetic-item",
                vec![LocalizedName {
                    locale: Locale::EnUs,
                    string_key: "synthetic-name".to_owned(),
                    text: "Synthetic Name".to_owned(),
                }],
            ),
        ] {
            let mut candidate = snapshot(vec![base.clone(), different]);
            candidate.sort_stably();
            assert_eq!(candidate.canonical_items.len(), 2);
        }
    }
}
