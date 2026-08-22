def fail($message): $message | error;

def normalize_name:
  gsub("０"; "0") | gsub("１"; "1") | gsub("２"; "2")
  | gsub("３"; "3") | gsub("４"; "4") | gsub("５"; "5")
  | gsub("６"; "6") | gsub("７"; "7") | gsub("８"; "8")
  | gsub("９"; "9") | ascii_downcase | gsub("[\\p{P}\\s]"; "");

def layer_sources($layer):
  if $layer == "official" then ["official"]
  elif $layer == "official_opencc" then ["official", "opencc"]
  else ["official", "opencc", "community"]
  end;

def match_candidates($candidates; $record):
  ($record.title | normalize_name) as $title
  | [$candidates[]
     | .normalized_name as $name
     | select($title | contains($name))
     | {id, source}];

def classify($matches; $layer):
  layer_sources($layer) as $sources
  | [$matches[] | select(.source as $source | $sources | index($source))] as $selected
  | ($selected | map(.id) | unique) as $ids
  | if ($ids | length) == 0 then {status: "unmatched", sources: []}
    elif ($ids | length) == 1 then
      {status: "resolved", canonical_id: $ids[0], sources: ($selected | map(.source) | unique)}
    else
      {status: "filtered_multi_item", sources: ($selected | map(.source) | unique)}
    end;

def aggregate($rows):
  {
    total: ($rows | length),
    eligible_total: ([$rows[] | select(.result.status != "filtered_multi_item")] | length),
    resolved: ([$rows[] | select(.result.status == "resolved")] | length),
    filtered_multi_item: ([$rows[] | select(.result.status == "filtered_multi_item")] | length),
    unmatched: ([$rows[] | select(.result.status == "unmatched")] | length),
    distinct_resolved_canonical_ids:
      ([$rows[] | select(.result.status == "resolved") | .result.canonical_id] | unique),
    resolved_source_matches: {
      official: ([$rows[] | select(.result.status == "resolved" and (.result.sources | index("official")))] | length),
      opencc: ([$rows[] | select(.result.status == "resolved" and (.result.sources | index("opencc")))] | length),
      community: ([$rows[] | select(.result.status == "resolved" and (.result.sources | index("community")))] | length)
    }
  };

def aggregate_family($rows; $family; $denominator):
  [$rows[] | select(.record.family == $family)] as $selected
  | {family: $family, denominator: $denominator}
    + aggregate($selected)
    + {categories:
        ([$selected[].record.category] | unique
         | map(. as $category
           | [$selected[] | select(.record.category == $category)] as $category_rows
           | {category: $category} + aggregate($category_rows)))};

def layer($matched_rows; $layer):
  [$matched_rows[] | {record, result: classify(.matches; $layer)}] as $rows
  | [aggregate_family($rows; "unique"; true),
     aggregate_family($rows; "set"; true),
     aggregate_family($rows; "mixed"; false)] as $families
  | [$rows[] | select(.record.family | IN("unique", "set"))] as $named
  | {
      layer: $layer,
      families: $families,
      named_page_denominator: ({families: ["unique", "set"]} + aggregate($named))
    };

if ($catalog | length) != 1 or ($corpus | length) != 1 then
  fail("exactly one catalog and corpus required")
elif ($catalog[0].catalog_version != 1)
  or (($catalog[0].candidate_groups | type) != "object")
  or any(["unique", "set", "mixed"][];
      ($catalog[0].candidate_groups[.] | type) != "array")
  or any($catalog[0].candidate_groups[][];
      (.id | type) != "string"
      or (.normalized_name | type) != "string"
      or .normalized_name == ""
      or (.source | IN("official", "opencc", "community") | not))
  or (($corpus[0].records | type) != "array") then
  fail("invalid inputs")
else
  $catalog[0] as $cat
  | $corpus[0] as $data
  | [$data.records[]
     | select(.family | IN("unique", "set", "mixed"))
     | . as $record
     | {record: $record, matches: match_candidates($cat.candidate_groups[$record.family]; $record)}] as $matched_rows
  | [$data.records[]
     | select(.family == "rune")
     | . as $record
     | ($record.rune_number | tonumber) as $number
     | ("base:r" + (if $number < 10 then "0" else "" end) + ($number | tostring)) as $id
     | if ($number < 1 or $number > 33) or ($cat.canonical_ids | index($id) | not)
       then fail("invalid rune ask")
       else {record: $record, canonical_id: $id}
       end] as $rune_rows
  | {
      report_version: 2,
      generated_at: $generated_at,
      catalog: {
        schema_version: $cat.snapshot.schema_version,
        product: $cat.snapshot.build.product,
        build_version: $cat.snapshot.build.version,
        canonical_item_count: $cat.snapshot.canonical_item_count,
        snapshot_sha256: $snapshot_sha256
      },
      variants: {
        opencc_version: $cat.opencc.version,
        opencc_config: $cat.opencc.config,
        alias_map_version: $cat.alias_map.version,
        alias_count: $cat.alias_map.count,
        alias_sha256: $alias_sha256
      },
      sample: {
        captured_at: $captured_at,
        request_count: ($request_count | tonumber),
        input_records: ($data.records | length),
        privacy_excluded: $data.privacy_excluded
      },
      layers: [layer($matched_rows; "official"),
               layer($matched_rows; "official_opencc"),
               layer($matched_rows; "official_opencc_community")],
      rune_taxonomy:
        [$data.rune_taxonomy[]
         | (.number | tonumber) as $number
         | ("base:r" + (if $number < 10 then "0" else "" end) + ($number | tostring)) as $id
         | if ($cat.canonical_ids | index($id) | not)
           then fail("invalid rune taxonomy")
           else {number: $number, category: .category, canonical_id: $id, status: "resolved"}
           end],
      rune_asks:
        ([$rune_rows[]
          | {category: .record.category,
             rune_number: (.record.rune_number | tonumber),
             canonical_id}]
         | group_by([.rune_number, .category, .canonical_id])
         | map({rune_number: .[0].rune_number,
                category: .[0].category,
                canonical_id: .[0].canonical_id,
                current_ask_records: length}))
    }
end
