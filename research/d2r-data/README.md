# D2R data evidence boundary

The committed fixture is synthetic and proves only the parser, canonical-ID,
locale, duplicate-affix, exact-item-deduplication, typed fixed and item-level
scaled charged-skill, BOM, numeric-range, and authored-alias behavior. It is not
a complete item database and does not establish permission to redistribute
Blizzard or third-party data.

The extractor whitelist is maintained in `crates/arreat-data/src/exporter.rs`.
It contains only the item, affix, property/stat/type/skill, and localization
inputs required by schema v1. Full exports and snapshots stay in ignored local
paths.

The scaled charged-skill fixture is authored and synthetic. It asserts that
negative source operands remain unchanged while the normalized formula inputs
are skill ID 900003, required level 24, five item levels per skill level, and
40 base charges. Evaluator boundary and charge-cap behavior is covered by Rust
tests; unequal records sharing an item identity remain an audit failure.

Incompatible changes increment `schema_version`. A field may be added as
optional only when every old snapshot keeps the same meaning.
