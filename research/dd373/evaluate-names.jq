def fail($message): $message | error;

def normalize_name:
  gsub("０"; "0") | gsub("１"; "1") | gsub("２"; "2")
  | gsub("３"; "3") | gsub("４"; "4") | gsub("５"; "5")
  | gsub("６"; "6") | gsub("７"; "7") | gsub("８"; "8")
  | gsub("９"; "9") | ascii_downcase | gsub("[\\p{P}\\s]"; "");

def allowed_id($family):
  if $family == "unique" then startswith("unique:")
  elif $family == "set" then startswith("set-item:")
  elif $family == "mixed" then startswith("unique:") or startswith("set-item:")
  else false
  end;

def candidates($catalog; $family):
  [
    $catalog.canonical_items[]
    | select(.id | allowed_id($family))
    | .id as $id
    | .names[]
    | select(.locale | IN("enUS", "zhTW", "zhCN"))
    | (.text | normalize_name) as $name
    | select($name != "")
    | {id: $id, name: $name, length: ($name | length)}
  ] | unique_by([.id, .name]);

def resolve($catalog; $record):
  ($record.title | normalize_name) as $title
  | [candidates($catalog; $record.family)[]
     | .name as $name
     | select($title | contains($name))] as $matches
  | if ($matches | length) == 0 then {status: "unmatched"}
    else ($matches | map(.length) | max) as $maximum
    | ($matches | map(select(.length == $maximum) | .id) | unique) as $ids
    | if ($ids | length) == 1
      then {status: "resolved", canonical_id: $ids[0]}
      else {status: "ambiguous"}
      end
    end;

def aggregate_category($rows; $category):
  [$rows[] | select(.record.category == $category)] as $selected
  | {
      category: $category,
      total: ($selected | length),
      resolved: ([$selected[] | select(.result.status == "resolved")] | length),
      ambiguous: ([$selected[] | select(.result.status == "ambiguous")] | length),
      unmatched: ([$selected[] | select(.result.status == "unmatched")] | length),
      distinct_resolved_canonical_ids:
        ([$selected[] | select(.result.status == "resolved") | .result.canonical_id] | unique)
    };

def aggregate_family($rows; $family; $denominator):
  [$rows[] | select(.record.family == $family)] as $selected
  | {
      family: $family,
      denominator: $denominator,
      total: ($selected | length),
      resolved: ([$selected[] | select(.result.status == "resolved")] | length),
      ambiguous: ([$selected[] | select(.result.status == "ambiguous")] | length),
      unmatched: ([$selected[] | select(.result.status == "unmatched")] | length),
      distinct_resolved_canonical_ids:
        ([$selected[] | select(.result.status == "resolved") | .result.canonical_id] | unique),
      categories:
        ([$selected[].record.category] | unique | map(aggregate_category($selected; .)))
    };

if ($catalog | length) != 1 or ($corpus | length) != 1 then
  fail("exactly one catalog and one corpus are required")
elif ($catalog[0].schema_version != 1)
  or (($catalog[0].canonical_items | type) != "array")
  or (($corpus[0].records | type) != "array") then
  fail("invalid Schema v1 catalog or sanitized corpus")
elif any($corpus[0].records[];
    (.sample_id | type) != "string"
    or (.family | IN("unique", "set", "mixed", "rune") | not)
    or (.category | type) != "string"
    or (.title | type) != "string") then
  fail("invalid sanitized record")
else
  $catalog[0] as $cat
  | $corpus[0] as $data
  | [
      $data.records[]
      | select(.family | IN("unique", "set", "mixed"))
      | {record: ., result: resolve($cat; .)}
    ] as $title_rows
  | [
      $data.records[]
      | select(.family == "rune")
      | . as $record
      | ($record.rune_number | tonumber) as $number
      | ("base:r" + (if $number < 10 then "0" else "" end) + ($number | tostring)) as $id
      | if ($number < 1 or $number > 33)
          or ([$cat.canonical_items[].id] | index($id) | not)
        then fail("rune ask does not map to an exact base:r01-base:r33 catalog item")
        else {record: $record, canonical_id: $id}
        end
    ] as $rune_rows
  | [aggregate_family($title_rows; "unique"; true),
     aggregate_family($title_rows; "set"; true),
     aggregate_family($title_rows; "mixed"; false)] as $families
  | {
      report_version: 1,
      generated_at: $generated_at,
      aliases_used: false,
      catalog: {
        schema_version: $cat.schema_version,
        product: $cat.build.product,
        build_version: $cat.build.version,
        canonical_item_count: ($cat.canonical_items | length),
        snapshot_sha256: $snapshot_sha256
      },
      sample: {
        captured_at: $captured_at,
        request_count: ($request_count | tonumber),
        input_records: ($data.records | length),
        privacy_excluded: $data.privacy_excluded
      },
      families: $families,
      named_resolution_denominator: {
        families: ["unique", "set"],
        total: ([$families[] | select(.denominator) | .total] | add),
        resolved: ([$families[] | select(.denominator) | .resolved] | add),
        ambiguous: ([$families[] | select(.denominator) | .ambiguous] | add),
        unmatched: ([$families[] | select(.denominator) | .unmatched] | add)
      },
      rune_taxonomy:
        [$data.rune_taxonomy[]
         | (.number | tonumber) as $number
         | ("base:r" + (if $number < 10 then "0" else "" end) + ($number | tostring)) as $id
         | if ($number < 1 or $number > 33)
             or ([$cat.canonical_items[].id] | index($id) | not)
           then fail("rune taxonomy does not map to an exact base:r01-base:r33 catalog item")
           else {number: $number, category: .category, canonical_id: $id, status: "resolved"}
           end],
      rune_asks:
        ([$rune_rows[] | {category: .record.category, rune_number: (.record.rune_number | tonumber), canonical_id}]
         | group_by([.rune_number, .category, .canonical_id])
         | map({rune_number: .[0].rune_number, category: .[0].category,
                canonical_id: .[0].canonical_id, current_ask_records: length}))
    }
end
