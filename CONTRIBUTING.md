# Contributing

Enter `nix develop`, then run the repository gates listed in CI. The fixture
workflow needs no D2R installation, network, SSH, or external data:

```console
mkdir -p .cache/arreat-index/fixture-a .cache/arreat-index/fixture-b
cargo run -p arreat-data -- normalize --input tests/fixtures/d2r-minimal --output .cache/arreat-index/fixture-a/snapshot.json
cargo run -p arreat-data -- normalize --input tests/fixtures/d2r-minimal --output .cache/arreat-index/fixture-b/snapshot.json
cmp .cache/arreat-index/fixture-a/snapshot.json .cache/arreat-index/fixture-b/snapshot.json
sha256sum .cache/arreat-index/fixture-a/snapshot.json .cache/arreat-index/fixture-b/snapshot.json
check-jsonschema --schemafile schemas/snapshot-v1.schema.json .cache/arreat-index/fixture-a/snapshot.json
cargo run -p arreat-data -- audit --snapshot .cache/arreat-index/fixture-a/snapshot.json --json .cache/arreat-index/fixture-a/audit.json --markdown .cache/arreat-index/fixture-a/audit.md
jq -e '
  ([.affixes[].modifiers[] | select(.interpretation.kind == "charged_skill")] | length == 1) and
  ([.affixes[].modifiers[] | select(.interpretation.kind == "scaled_charged_skill")] | length == 1) and
  any(.affixes[].modifiers[];
    .source_operands == {parameter: 900003, min: -40, max: -15} and
    .interpretation == {kind: "scaled_charged_skill", skill_id: 900003,
      skill_required_level: 24, item_levels_per_skill_level: 5, base_charges: 40})
' .cache/arreat-index/fixture-a/snapshot.json
jq -e '
  .passed and .error_count == 0 and .gap_count == 0 and
  ([.warlock_sentinels[]] | all)
' .cache/arreat-index/fixture-a/audit.json
```

Rust tests additionally assert item-level evaluator boundaries and the 255
charge cap, exact duplicate-item collapse, and fatal unequal same-ID items.

For local full-data work, build the runnable Nix closure with
`nix build .#arreat-data-static`, run `export` against your own
read-only game root into `exports/arreat-index-<build>.tar`, then normalize and
audit into `snapshots/arreat-index-full-<build>.json`. These paths are ignored.
Review applicable terms and never upload source tables, archives, full
snapshots, credentials, or third-party catalogs.

Real acceptance is deferred. After an approved checkpoint, the main agent uses
an explicit dedicated SSH identity, passed with `BatchMode` and
`IdentitiesOnly yes`; no SSH alias is required. It copies the exact package
closure with `nix copy`, not
a standalone binary, and discovers the install root read-only. Never record an
IP, password, credential path/value, or absolute game path in this repository.

The source whitelist is `SOURCE_WHITELIST` in the exporter. Incompatible
format changes increment `schema_version`; additions are optional only when old
snapshots retain their meaning. Fixture or schema updates require reviewing the
deterministic diff and audit report.
