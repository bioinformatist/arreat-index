# Product Boundaries

- A market scope combines a season scope (non-season or latest season) with a
  play mode (normal or hardcore). `新赛季` and `赛季` both mean latest season.
- DD373 support is limited to `非赛季(术士君临)`,
  `非赛季专家(术士君临)`, `新赛季(术士君临)` / `赛季(术士君临)`, and
  `新赛季专家(术士君临)` / `赛季专家(术士君临)`.
- Pre-`术士君临` servers are unsupported. Never select a legacy server or use
  one as a fallback.
- Initial testing defaults to non-season normal, while explicit season and
  play-mode choices must remain available for later UI use.
- An unavailable supported market scope and a scope with no comparable current
  asks are market states. Malformed or ambiguous provider taxonomy is a
  provider failure.
