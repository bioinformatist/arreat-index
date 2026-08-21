# D2R data evidence boundary

The committed fixture is synthetic and proves only the parser, canonical-ID,
locale, duplicate-affix, typed modifier, BOM, numeric-range, and authored-alias behavior. It is not
a complete item database and does not establish permission to redistribute
Blizzard or third-party data.

The extractor whitelist is maintained in `crates/arreat-data/src/exporter.rs`.
It contains only the item, affix, property/stat/type/skill, and localization
inputs required by schema v1. Full exports and snapshots stay in ignored local
paths.

Incompatible changes increment `schema_version`. A field may be added as
optional only when every old snapshot keeps the same meaning.
