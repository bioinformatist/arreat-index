# Store raw property operands with a typed interpretation

Schema v1 stores every Affix Modifier's Source Operands alongside exactly one tagged Modifier Interpretation. The normalizer owns classification because it already combines affix rows with property and skill metadata behind its small public interface.

Universal ranges were rejected because Min and Max can encode different quantities, and raw-only output was rejected because every consumer would otherwise repeat subtle metadata interpretation. Unknown metadata therefore remains lossless as an Uninterpreted Modifier and produces an explicit audit gap instead of a guess or normalization failure. This unreleased v1 decision keeps provenance and derived meaning together before the public format becomes costly to reverse.
