# DD373 feasibility snapshot

Captured 2026-08-13 with 19 unauthenticated, read-only GET requests at no more
than one request per second. Full response bodies stayed in a temporary
directory and were deleted; this pack retains provenance plus three normalized,
manually reviewed samples. A `URL; sha256` pair below identifies a source in
`manifest.json`.

## current_listing_status

**available_with_gaps.** The official listing document describes a current
ratio-goods endpoint and its price, quantity, unit, and trade fields
(`https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html`;
`2264820320f15861114f3e8a6776d749afb059f193d3e7ac2b2cd855aafe1517`).
For the dynamically discovered D2R mainland game, one non-season ordinary realm
query returned 7 rune asks, while the named-weapon and random-affix-ring queries
each returned 30 asks. These are response/page limits observed for these exact
queries, not completeness guarantees. The corresponding listing response hashes
are `71a6c3e64b33aa0bf31b62e2b9339f0bb827ac7391d0cb93516817bed8cbd377`,
`6a53b800dd7abfabe06026e00a710ff2c6e268ab8ae5a8ff89db95509568c59f`,
and `f361eb408ca4946d9a9a7b61f9ed4b1b1d8129bb517fc08b1b440f0b8b46c5f7`;
their full URLs are in the manifest. Prices are **当前挂单**, not verified
transactions.

## item_identity_status

**partial.** The game list returned `暗黑2：重制版国服` with `IsClose=false`,
`CanTrade=true`, and `IsEnabled=true`
(`https://game.dd373.com/api/game/list`;
`5812c02bda1a653f0b32dfd0f0ceeab8f1668c1e5a6f953aad3f08b868fbfcc3`).
The route exposes game, goods type, goods subtype, region, and server levels
(`https://game.dd373.com/Api/GameRoute/List?gameId=c97d2193c0a445b6a717acb6fdb17c16`;
`8098399038d0242498d35bc59d74df5844cb23502d5325ea6a225aae39eaddee`).
The goods hierarchy structurally distinguishes 33 numbered runes, but named
equipment is classified only to broad classes such as unique weapon; its exact
item identity and variants remain free text
(`https://game.dd373.com/Api/GameGoodsType/List?parentId=8405dd8baaa5498ab86ec2a7073282b4`;
`af119e83b4221edfe80dd6165f7547192c3c5ccc7ab1632d1a8c76914f08210e`;
`https://game.dd373.com/Api/GameGoodsType/List?parentId=d8713381cd354aa3a1bea74e1fe23ebe`;
`56e827fc43142b3a222ac0f9ee3501f71d2cb4bcba73910c17901ff33a439040`).

Realm is structurally `国服`; server names vary on season wording, expert
presence, and two expansion labels. Ladder is represented only through season
wording. Platform is absent, and no separate mode field was observed. These are
observed labels, not inferred equivalences:
(`https://game.dd373.com/Api/GameOther/List?parentId=7b1751f92c844871ab80cae0822feea2`;
`587a2258c1a4e5d6fcd38cbcb2e70293df3e873e6c8b634c87e39281cc507df5`).

DD373 labels are provider-facing and are not official Blizzard terms; project
mapping keeps the axes explicit:

| Project concept | Game axis or object | DD373 label/field | Evidence status |
|---|---|---|---|
| Fixed Era | `术士君临` | `非赛季(术士君临)`, `非赛季专家(术士君临)`, `新赛季(术士君临)`, `赛季(术士君临)`, `新赛季专家(术士君临)`, `赛季专家(术士君临)` | observed |
| Market Scope | 非天梯 + 标准 | `非赛季(术士君临)` | observed |
| Market Scope | 非天梯 + 专家模式 | `非赛季专家(术士君临)` | observed |
| Market Scope | 天梯 + 标准 | `新赛季(术士君临)` or `赛季(术士君临)` | observed |
| Market Scope | 天梯 + 专家模式 | `新赛季专家(术士君临)` or `赛季专家(术士君临)` | observed |
| Per-item Ask | Rune | `price`, `amount`, `singleprice` | helpdetail ratio text: `dc862c7b70d74880968d2386e544b5bf.html`; committed rune samples: `price-semantics-report.json` |
| Per-listing Ask | Fixed-name Unique or Set item | `price`, observed empty `unit` | observed with `unique:The Oculus` and `set-item:Tal Rasha's Adjudication` in `price-semantics-report.json` and `price-semantics-manifest.json` |

`专家` is present in expert labels and absent from normal labels.

The normal-market project scope currently uses non-expert DD373 labels:
`非赛季(术士君临)`, `新赛季(术士君临)`, and `赛季(术士君临)`.

DD373 exposes price fields and order forms in its official article
(`https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html`):
`price`, `amount`, and `singleprice` (`单价比例`) are the observed ratio terms. The
family-aware feasibility evidence is limited to three fixed samples recorded in
`price-semantics-report.json`, with provenance in
`price-semantics-manifest.json`, and is scoped to
`base:r17`, `unique:The Oculus`, and `set-item:Tal Rasha's Adjudication`.

## random_affix_status

**free_text_only.** Random-affix goods are structurally classified only as
`亮金装备&饰品` and a broad equipment slot. Affix identity and numeric values
appear in the title, not distinct response fields. The listing does structure
trade mode, lot count, quantity, price, and unit (empty for the sampled ring)
(`https://game.dd373.com/Api/GameGoodsType/List?parentId=43f21d3cf4874007956939960e88db72`;
`2c4879c7cf810d67d259d91132fe3e4655acf73d13a4a49e5bd9d694b0546387`;
random listing URL in the manifest;
`f361eb408ca4946d9a9a7b61f9ed4b1b1d8129bb517fc08b1b440f0b8b46c5f7`).

## history_status

**not_found_in_official_docs.** The official game-interface index enumerates
game, route, region/server, and goods-type endpoints, but no historical listing
or transaction endpoint
(`https://kf.dd373.com/helpdetail/e5675fb48f6a42e4a59ae5712b132965.html`;
`93b59ddd2c1e9ca6bcc3c727fcdaa0fa5fefbce4ceb8eab1a5355a01d65d6270`).
The official listing document describes ordinary, mall, and purchase-request
current lists, likewise without a historical source
(`https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html`;
`2264820320f15861114f3e8a6776d749afb059f193d3e7ac2b2cd855aafe1517`).
No polling-derived series was created.

## access_and_terms_status

**public_access_observed_terms_unknown.** All 19 requests returned HTTP 200
without authentication, a challenge, or observed rate-limit/error response.
That demonstrates access only at capture time, not future stability. The linked
user-agreement navigation and disclaimer were reviewed; no express prohibition
of this bounded research was found, but neither source establishes a perpetual
redistribution license
(`https://kf.dd373.com/helpdetail/78dba8ca31b44712aef8e923f6c61984.html`;
`cdeed520977b0245ad3a7f359c94f85c0f4953ff95a9c15b960d4ae40e4742c9`;
`https://about.dd373.com/Index.html?catenum=3732&childcatenum=37325`;
`9dd2a83f896be56ef0ddd5587011e54f938f47b36d53fcb144945409e576b883`).

## conclusion

**CONDITIONAL.** Public, challenge-free current asks cover standardized, named,
and random-affix families, but named identity and affixes are free text, history
is unverified, endpoint stability is only a dated observation, and redistribution
permission is unresolved. The user must separately decide matching semantics,
technical boundaries, data policy, history semantics, and initial item scope.
This evidence pack makes none of those design choices.

## Layered name-matching snapshot (2026-08-22)

**CONDITIONAL.** The accepted replacement capture made 32 unauthenticated,
read-only HTTPS requests, all HTTP 200, with starts at least 1115 ms apart. It
produced 557 current-listing records, including 478 records from unique and set
leaf pages. The preceding capture attempt timed out and was discarded. Two
later GETs used only to diagnose network routing were also excluded from the
corpus, with no response or title data retained. Neither the failed attempt nor
the diagnostics are corpus evidence.

Plan 010 mechanically classified 479 unique/set page records as 182 resolved,
3 ambiguous, and 294 unmatched using official names. The fresh capture uses a
repaired 1,428-item D2R 3.3.93854 catalog and three cumulative layers over the
same 478 records:

| layer | total | eligible | resolved | filtered multi-item | unmatched |
|---|---:|---:|---:|---:|---:|
| official | 478 | 468 | 160 | 10 | 308 |
| official + OpenCC | 478 | 467 | 200 | 11 | 267 |
| official + OpenCC + community map | 478 | 433 | 284 | 45 | 149 |

Unique pages remain restricted to unique items and set pages to set items.
When a title matches more than one canonical ID, it is classified as filtered
multi-item and removed from the eligible denominator; therefore eligible is
`total - filtered multi-item`. OpenCC 1.3.0 uses `tw2s.json`, and the bounded
community map contains 87 pre-approved aliases. The separate rune taxonomy
still contains exactly 33 entries, and all 19 sampled rune asks mapped exactly.

These are deterministic matching counts for current asks, not accuracy,
precision, recall, completeness, transaction history, or production-feasibility
claims. Endpoint stability and redistribution rights remain unresolved. Only
aggregate results and request provenance are retained: no raw bodies, listing
or normalized titles, source rows, seller/account/contact/price data, generated
snapshot, full catalog, or private cache path is published.

## Family-aware price-semantics snapshot (2026-08-23)

**SUPPORTED for the three fixed samples in this bounded capture.** One
unauthenticated, read-only 13-request capture examined non-season normal play on
the exact `非赛季(术士君临)` server. Request starts were at least 1119 ms apart,
all responses were HTTP 200, redirects and retries were disabled, and no raw
response body was retained. The fixed samples were `base:r17`,
`unique:The Oculus`, and `set-item:Tal Rasha's Adjudication`; no substitute
item, taxonomy leaf, scope, or server was used.

The rune page had 23 matched rows. All had positive numeric `price`, `amount`,
and `singleprice` fields, and their aggregate relation classification supported
`price / amount == singleprice` within the report's relative tolerance. The
unique page had 13 matched Oculus rows, all with positive listing prices and
empty units. The set page had one matched Tal Rasha's Adjudication row with a
positive listing price and an empty unit. Thus the machine-derived reason is
`price_over_amount_direction_observed` for the rune and
`positive_listing_price_with_empty_unit` for both named-item families. The
complete aggregate counts and relation classes are in
`price-semantics-report.json`; request provenance is in
`price-semantics-manifest.json`.

The interpretation was cross-checked against DD373's official ratio-goods and
order-field documentation and against three pinned community implementations:
`HuskyCommunicator/d2r-price-qq-bot@cebecdf5a340a4fc00132bca663f8b263041ac9c`,
`lhe6330-cloud/d2r-equipment-checker@60bb917729acee194485ed81d16048cadd0c4aef`,
and
`SirYuxuan/astrbot-plugin-dnf@108e0f98c68ec671b6c108ff6492b698284d72f2`.
Those sources are evidence only: no community code, hard-coded route,
dependency, browser behavior, or alias was imported. The two D2R repositories
had no detected license at the inspected revisions; the DNF repository was
AGPL-3.0.

This result is a dated observation of current asks, not a completeness,
transaction-price, endpoint-stability, or redistribution-rights guarantee. It
does not establish semantics for other items, families, seasons, hardcore
markets, or servers. In particular, an empty equipment unit is not replaced by
an invented unit. This research selects no production price basis, public field
name, schema version, aggregation change, provider seam, or UI behavior; those
remain decisions for a later plan.
