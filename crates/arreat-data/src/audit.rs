use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{Error, FindingSeverity, Locale, ModifierInterpretation, Result, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    pub schema_version: u32,
    pub passed: bool,
    pub item_count: usize,
    pub affix_count: usize,
    pub alias_count: usize,
    pub locale_coverage: BTreeMap<String, usize>,
    pub error_count: usize,
    pub gap_count: usize,
    pub informational_count: usize,
    pub duplicate_item_identities: Vec<String>,
    pub duplicate_affix_identities: Vec<String>,
    pub duplicate_display_names: BTreeMap<String, Vec<String>>,
    pub warlock_sentinels: BTreeMap<String, bool>,
    pub findings: Vec<crate::AuditFinding>,
}

pub fn audit_snapshot(snapshot: &Snapshot) -> AuditReport {
    let mut locale_coverage = BTreeMap::from([
        ("enUS".to_owned(), 0),
        ("zhTW".to_owned(), 0),
        ("zhCN".to_owned(), 0),
    ]);
    for name in snapshot
        .canonical_items
        .iter()
        .flat_map(|item| &item.names)
        .chain(snapshot.affixes.iter().flat_map(|affix| &affix.names))
    {
        let key = match name.locale {
            Locale::EnUs => "enUS",
            Locale::ZhTw => "zhTW",
            Locale::ZhCn => "zhCN",
        };
        *locale_coverage.get_mut(key).expect("known locale") += 1;
    }
    let duplicate_item_identities = duplicates(
        snapshot
            .canonical_items
            .iter()
            .map(|item| item.id.to_string()),
    );
    let duplicate_affix_identities =
        duplicates(snapshot.affixes.iter().map(|affix| affix.id.to_string()));
    let mut display_to_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for affix in &snapshot.affixes {
        for name in &affix.names {
            display_to_ids
                .entry(format!("{:?}:{}", name.locale, name.text))
                .or_default()
                .push(affix.id.to_string());
        }
    }
    display_to_ids.retain(|_, ids| {
        ids.sort();
        ids.dedup();
        ids.len() > 1
    });
    let wa1 = snapshot.canonical_items.iter().any(|item| {
        item.id.to_string() == "base:wa1"
            && has_names(
                item.names.iter().map(|name| name.text.as_str()),
                &["Old Book", "舊書", "古书"],
            )
    });
    let ars = snapshot.canonical_items.iter().any(|item| {
        item.names
            .iter()
            .any(|name| name.string_key == "Ars Al'Diablolos")
            && has_names(
                item.names.iter().map(|name| name.text.as_str()),
                &["Ars Al'Diabolos", "艾迪亞布羅斯學術", "迪亚波罗斯之术"],
            )
    });
    let chaotic_ids = snapshot
        .affixes
        .iter()
        .filter(|affix| affix.names.iter().any(|name| name.text == "Chaotic"))
        .map(|affix| affix.id.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let variable = snapshot
        .affixes
        .iter()
        .flat_map(|affix| &affix.modifiers)
        .any(|modifier| {
            matches!(
                modifier.interpretation,
                ModifierInterpretation::NumericRange { minimum, maximum }
                    if minimum < maximum
            )
        });
    let aliases = snapshot
        .aliases
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut counts, alias| {
            *counts
                .entry(alias.canonical_item_id.to_string())
                .or_default() += 1;
            counts
        })
        .values()
        .any(|count| *count >= 2);
    let warlock_sentinels = BTreeMap::from([
        ("ars_al_diablolos_locales".to_owned(), ars),
        ("authored_aliases_share_item".to_owned(), aliases),
        ("chaotic_ids_distinct".to_owned(), chaotic_ids.len() >= 2),
        ("variable_roll_preserved".to_owned(), variable),
        ("wa1_locales".to_owned(), wa1),
    ]);
    let error_count = snapshot
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Error)
        .count();
    let gap_count = snapshot
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Gap)
        .count();
    let informational_count = snapshot
        .findings
        .iter()
        .filter(|finding| finding.severity == FindingSeverity::Info)
        .count();
    let passed = error_count == 0
        && duplicate_item_identities.is_empty()
        && duplicate_affix_identities.is_empty();
    AuditReport {
        schema_version: snapshot.schema_version,
        passed,
        item_count: snapshot.canonical_items.len(),
        affix_count: snapshot.affixes.len(),
        alias_count: snapshot.aliases.len(),
        locale_coverage,
        error_count,
        gap_count,
        informational_count,
        duplicate_item_identities,
        duplicate_affix_identities,
        duplicate_display_names: display_to_ids,
        warlock_sentinels,
        findings: snapshot.findings.clone(),
    }
}

pub fn write_audit(
    snapshot_path: &Path,
    json_path: &Path,
    markdown_path: &Path,
) -> Result<AuditReport> {
    let bytes =
        fs::read(snapshot_path).map_err(|source| crate::error::io(snapshot_path, source))?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: snapshot_path.display().to_string(),
        source,
    })?;
    let report = audit_snapshot(&snapshot);
    let mut json = serde_json::to_vec_pretty(&report).map_err(|source| Error::Json {
        path: json_path.display().to_string(),
        source,
    })?;
    json.push(b'\n');
    fs::write(json_path, json).map_err(|source| crate::error::io(json_path, source))?;
    let sentinels = report
        .warlock_sentinels
        .iter()
        .map(|(name, passed)| format!("- {}: {}", name, if *passed { "PASS" } else { "FAIL" }))
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = format!(
        "# D2R data audit\n\nResult: **{}**\n\n- Items: {}\n- Affixes: {}\n- Aliases: {}\n- Errors: {}\n- Explicit gaps: {}\n- Informational findings: {}\n\n## Required sentinels\n\n{}\n",
        if report.passed { "PASS" } else { "FAIL" },
        report.item_count,
        report.affix_count,
        report.alias_count,
        report.error_count,
        report.gap_count,
        report.informational_count,
        sentinels
    );
    fs::write(markdown_path, markdown).map_err(|source| crate::error::io(markdown_path, source))?;
    Ok(report)
}

fn duplicates(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0_usize) += 1;
    }
    counts
        .into_iter()
        .filter_map(|(value, count)| (count > 1).then_some(value))
        .collect()
}

fn has_names<'a>(actual: impl Iterator<Item = &'a str>, expected: &[&str]) -> bool {
    let actual = actual.collect::<std::collections::BTreeSet<_>>();
    expected.iter().all(|name| actual.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuditFinding, CanonicalItem, CanonicalItemId, GameBuild, ItemKind};

    fn empty_snapshot() -> Snapshot {
        Snapshot {
            schema_version: 1,
            build: GameBuild {
                product: "fixture".to_owned(),
                build_key: "fixture".to_owned(),
                version: "fixture".to_owned(),
                input_sha256: Vec::new(),
            },
            canonical_items: Vec::new(),
            affixes: Vec::new(),
            aliases: Vec::new(),
            findings: Vec::new(),
        }
    }

    #[test]
    fn evidence_sentinels_do_not_determine_integrity() {
        let report = audit_snapshot(&empty_snapshot());

        assert!(report.warlock_sentinels.values().all(|value| !value));
        assert!(report.passed);
    }

    #[test]
    fn an_error_fails_integrity() {
        let mut snapshot = empty_snapshot();
        snapshot.findings.push(AuditFinding {
            severity: FindingSeverity::Error,
            code: "fixture".to_owned(),
            reference: "fixture".to_owned(),
            message: "fixture".to_owned(),
        });

        assert!(!audit_snapshot(&snapshot).passed);
    }

    #[test]
    fn unequal_items_with_the_same_identity_remain_fatal() {
        let mut snapshot = empty_snapshot();
        let id = CanonicalItemId {
            kind: ItemKind::Unique,
            source_key: "synthetic-item".to_owned(),
        };
        snapshot.canonical_items = vec![
            CanonicalItem {
                id: id.clone(),
                source_table: "uniqueitems.txt".to_owned(),
                source_key: "synthetic-item".to_owned(),
                names: Vec::new(),
            },
            CanonicalItem {
                id,
                source_table: "setitems.txt".to_owned(),
                source_key: "synthetic-item".to_owned(),
                names: Vec::new(),
            },
        ];
        snapshot.sort_stably();

        let report = audit_snapshot(&snapshot);
        assert_eq!(report.duplicate_item_identities, ["unique:synthetic-item"]);
        assert!(!report.passed);
    }
}
