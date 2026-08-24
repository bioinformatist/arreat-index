def fail($message): $message | error;

def normalize_name:
  gsub("０"; "0") | gsub("１"; "1") | gsub("２"; "2")
  | gsub("３"; "3") | gsub("４"; "4") | gsub("５"; "5")
  | gsub("６"; "6") | gsub("７"; "7") | gsub("８"; "8")
  | gsub("９"; "9") | ascii_downcase | gsub("[\\p{P}\\s]"; "");

def forbidden_title:
  test("[[:alnum:]._%+-]+@[[:alnum:].-]+\\.[[:alpha:]]{2,}"; "i")
  or test("https?://|www\\."; "i")
  or test("[0-9０-９]{5,}")
  or test("qq|微信|vx|v信|telegram|discord|手机号|电话"; "i");

def zero_code:
  . == null or . == 0 or . == "0";

def decimal_text:
  if type == "string" then .
  elif type == "number" then tostring
  else null
  end;

def decimal_kind($value):
  if $value == null then "missing"
  else ($value | decimal_text) as $text
    | if $text == null or ($text | test("^-?[0-9]+(?:\\.[0-9]+)?$") | not)
      then "non_numeric"
      elif ($text | tonumber) > 0 then "positive"
      else "non_positive"
      end
  end;

def positive_number($value):
  ($value | decimal_text) as $text
  | if $text != null and ($text | test("^-?[0-9]+(?:\\.[0-9]+)?$")) and (($text | tonumber) > 0)
    then ($text | tonumber)
    else null
    end;

def field_counts($rows; $field):
  [$rows[] | decimal_kind(.[$field])] as $kinds
  | {
      positive: ([$kinds[] | select(. == "positive")] | length),
      missing: ([$kinds[] | select(. == "missing")] | length),
      non_numeric: ([$kinds[] | select(. == "non_numeric")] | length),
      non_positive: ([$kinds[] | select(. == "non_positive")] | length)
    };

def approximately($left; $right; $tolerance):
  (($left - $right) | fabs) <= ($tolerance * ([($left | fabs), ($right | fabs), 1] | max));

def relation($record; $tolerance):
  positive_number($record.price) as $price
  | positive_number($record.singleprice) as $singleprice
  | positive_number($record.amount) as $amount
  | if $price == null or $singleprice == null or $amount == null then
      "neither_or_insufficient"
    elif approximately($amount; 1; $tolerance) then
      "unit_quantity_ambiguous"
    else
      approximately(($price / $amount); $singleprice; $tolerance) as $forward
      | approximately(($amount / $price); $singleprice; $tolerance) as $reverse
      | if $forward and ($reverse | not) then "price_over_amount_matches_singleprice"
        elif $reverse and ($forward | not) then "amount_over_price_matches_singleprice"
        else "neither_or_insufficient"
        end
    end;

def conclusion($family; $counts; $relations):
  if $counts.matched == 0 then
    {status: "inconclusive", reason: "no_matched_rows"}
  elif $family == "rune" then
    if $relations.price_over_amount_matches_singleprice > 0
       and $relations.amount_over_price_matches_singleprice == 0 then
      {status: "supported", reason: "price_over_amount_direction_observed"}
    elif $relations.amount_over_price_matches_singleprice > 0
         and $relations.price_over_amount_matches_singleprice == 0 then
      {status: "supported", reason: "amount_over_price_direction_observed"}
    elif $relations.price_over_amount_matches_singleprice > 0
         and $relations.amount_over_price_matches_singleprice > 0 then
      {status: "contradicted", reason: "mixed_ratio_directions"}
    elif $relations.unit_quantity_ambiguous > 0
         and $relations.neither_or_insufficient == 0 then
      {status: "inconclusive", reason: "unit_quantity_only"}
    else
      {status: "inconclusive", reason: "insufficient_ratio_evidence"}
    end
  elif $counts.price.positive == 0 then
    {status: "contradicted", reason: "no_positive_listing_price"}
  elif $counts.price.positive != $counts.matched then
    {status: "inconclusive", reason: "mixed_listing_price_availability"}
  elif $counts.unit.empty == 0 then
    {status: "inconclusive", reason: "no_empty_unit_observed"}
  else
    {status: "supported", reason: "positive_listing_price_with_empty_unit"}
  end;

if ($catalog | length) != 1 or ($response | length) != 1 then
  fail("exactly one catalog and response are required")
elif ($family | IN("rune", "unique", "set-item") | not)
  or (($family == "rune" and $canonical_id != "base:r17")
      or ($family == "unique" and $canonical_id != "unique:The Oculus")
      or ($family == "set-item" and $canonical_id != "set-item:Tal Rasha's Adjudication")) then
  fail("unexpected family or canonical ID")
elif $catalog[0].catalog_version != 1
  or ($catalog[0].canonical_ids | type) != "array"
  or ($catalog[0].canonical_ids | index($canonical_id)) == null
  or ($catalog[0].candidate_groups | type) != "object"
  or any(["unique", "set"][]; ($catalog[0].candidate_groups[.] | type) != "array")
  or any($catalog[0].candidate_groups.unique[], $catalog[0].candidate_groups.set[];
      (.id | type) != "string"
      or (.normalized_name | type) != "string"
      or .normalized_name == ""
      or (.source | IN("official", "opencc", "community") | not)) then
  fail("invalid catalog")
elif ($response[0] | type) != "object"
  or (($response[0].StatusCode // null) | zero_code | not)
  or ($response[0].StatusData | type) != "object"
  or (($response[0].StatusData.ResultCode // null) | zero_code | not)
  or ($response[0].StatusData.ResultData | type) != "array"
  or any($response[0].StatusData.ResultData[]; type != "object" or (.title | type) != "string") then
  fail("invalid listing response shape")
else
  $catalog[0] as $cat
  | $response[0].StatusData.ResultData as $records
  | (if $family == "unique" then $cat.candidate_groups.unique
     elif $family == "set-item" then $cat.candidate_groups.set
     else []
     end) as $candidates
  | [$records[]
     | . as $record
     | if ($record.title | forbidden_title) then
         {status: "privacy"}
       elif $family == "rune" then
         {status: "identity_eligible", record: $record}
       else
         ($record.title | normalize_name) as $title
         | [$candidates[] | . as $candidate | select($title | contains($candidate.normalized_name)) | .id] | unique as $ids
         | if ($ids | length) > 1 then {status: "multi_item"}
           elif ($ids | length) != 1 or $ids[0] != $canonical_id then {status: "unmatched"}
           else {status: "identity_eligible", record: $record}
           end
       end] as $classified
  | [$classified[] | select(.status == "identity_eligible") | .record] as $identity_eligible
  | reduce $identity_eligible[] as $record
      ({seen: [], kept: [], duplicates: 0};
       ($record.shopno // null) as $shopno
       | if (($shopno | type) | IN("string", "number") | not) or (($shopno | tostring) == "") then
           fail("matched record has invalid listing ID")
         elif (.seen | index($shopno | tostring)) != null then
           .duplicates += 1
         else
           .seen += [($shopno | tostring)] | .kept += [$record]
         end) as $deduplicated
  | $deduplicated.kept as $matched
  | {
      records_seen: ($records | length),
      privacy_excluded: ([$classified[] | select(.status == "privacy")] | length),
      multi_item_excluded: ([$classified[] | select(.status == "multi_item")] | length),
      unmatched_excluded: ([$classified[] | select(.status == "unmatched")] | length),
      duplicate_excluded: $deduplicated.duplicates,
      matched: ($matched | length),
      price: field_counts($matched; "price"),
      singleprice: field_counts($matched; "singleprice"),
      amount: field_counts($matched; "amount"),
      unit: {
        empty: ([$matched[] | select((.unit | type) != "string" or (.unit | gsub("^\\s+|\\s+$"; "")) == "")] | length),
        nonempty: ([$matched[] | select((.unit | type) == "string" and (.unit | gsub("^\\s+|\\s+$"; "")) != "")] | length)
      }
    } as $counts
  | (if $family == "rune" then
       [$matched[] | relation(.; $relative_tolerance)]
     else [] end) as $relation_rows
  | {
      family: $family,
      canonical_id: $canonical_id,
      counts: $counts,
      rune_relations: {
        price_over_amount_matches_singleprice:
          ([$relation_rows[] | select(. == "price_over_amount_matches_singleprice")] | length),
        amount_over_price_matches_singleprice:
          ([$relation_rows[] | select(. == "amount_over_price_matches_singleprice")] | length),
        unit_quantity_ambiguous:
          ([$relation_rows[] | select(. == "unit_quantity_ambiguous")] | length),
        neither_or_insufficient:
          ([$relation_rows[] | select(. == "neither_or_insufficient")] | length)
      }
    }
  | . + {conclusion: conclusion($family; .counts; .rune_relations)}
end

