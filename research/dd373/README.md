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

Realm is structurally `国服`; server names distinguish season/non-season,
ordinary/expert (hardcore), and two expansion labels. Ladder is represented only
through the season wording. Platform is absent, and no separate mode field was
observed. These are observed labels, not inferred equivalences
(`https://game.dd373.com/Api/GameOther/List?parentId=7b1751f92c844871ab80cae0822feea2`;
`587a2258c1a4e5d6fcd38cbcb2e70293df3e873e6c8b634c87e39281cc507df5`).

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
