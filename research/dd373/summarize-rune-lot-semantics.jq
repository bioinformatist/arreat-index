def fail($message): $message | error;

def zero_code:
  . == 0 or . == "0";

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
  | if $text != null
       and ($text | test("^-?[0-9]+(?:\\.[0-9]+)?$"))
       and (($text | tonumber) > 0)
    then ($text | tonumber)
    else null
    end;

def positive_integral($value):
  positive_number($value) as $number
  | if $number != null and ($number | floor) == $number then $number else null end;

def field_counts($rows; $field; $integral):
  [$rows[] | decimal_kind(.[$field])] as $kinds
  | {
      positive: ([$kinds[] | select(. == "positive")] | length),
      missing: ([$kinds[] | select(. == "missing")] | length),
      non_numeric: ([$kinds[] | select(. == "non_numeric")] | length),
      non_positive: ([$kinds[] | select(. == "non_positive")] | length)
    }
  | if $integral then
      . + {
        positive_integral: ([$rows[] | select(positive_integral(.[$field]) != null)] | length),
        positive_non_integral: ([$rows[]
          | select(positive_number(.[$field]) != null and positive_integral(.[$field]) == null)] | length)
      }
    else . end;

def approximately($left; $right; $tolerance):
  (($left - $right) | fabs)
    <= ($tolerance * ([($left | fabs), ($right | fabs), 1] | max));

def forbidden_text:
  test("[[:alnum:]._%+-]+@[[:alnum:].-]+\\.[[:alpha:]]{2,}"; "i")
  or test("https?://|www\\."; "i")
  or test("[0-9０-９]{5,}")
  or test("qq|微信|vx|v信|telegram|discord|手机号|电话"; "i");

def privacy_bearing:
  ((.title | forbidden_text))
  or any(to_entries[];
    (.key | test("^(seller|contact|account|memo|description|detail_?url)$"; "i"))
    and (.value != null and .value != ""));

def tuple($record; $tolerance):
  positive_number($record.price) as $price
  | positive_number($record.singleprice) as $singleprice
  | positive_integral($record.amount) as $amount
  | positive_integral($record.number) as $number
  | if $price == null or $singleprice == null or $amount == null or $number == null then
      {relation: "insufficient"}
    elif approximately(($price / $amount); $singleprice; $tolerance) then
      {
        relation: "matches",
        identity: ($record.shopno | tostring),
        price: $price,
        singleprice: $singleprice,
        amount: $amount
      }
    else
      {relation: "contradicts"}
    end;

def quantity_bin($amount):
  if $amount == 1 then "one"
  elif $amount < 10 then "two_to_nine"
  elif $amount < 100 then "ten_to_ninety_nine"
  else "hundred_or_more"
  end;

def set_class($tuples):
  if ($tuples | length) == 0 then "not_comparable"
  elif all($tuples[]; .amount == 1) then "single_only"
  elif all($tuples[]; .amount > 1) then "grouped_only"
  else "mixed"
  end;

def minima($tuples; $tolerance):
  if ($tuples | length) == 0 then
    {
      unit_minimum: {tie_count: 0, set_class: "not_comparable"},
      entry_minimum: {tie_count: 0, set_class: "not_comparable"},
      relationship: "not_comparable"
    }
  else
    ($tuples | map(.singleprice) | min) as $unit_minimum
    | ($tuples | map(.price) | min) as $entry_minimum
    | [$tuples[] | select(approximately(.singleprice; $unit_minimum; $tolerance))] as $unit_set
    | [$tuples[] | select(approximately(.price; $entry_minimum; $tolerance))] as $entry_set
    | ([$unit_set[].identity] | unique) as $unit_ids
    | ([$entry_set[].identity] | unique) as $entry_ids
    | ([$unit_ids[] | select(. as $id | $entry_ids | index($id) != null)] | length) as $overlap
    | {
        unit_minimum: {
          tie_count: ($unit_set | length),
          set_class: set_class($unit_set)
        },
        entry_minimum: {
          tie_count: ($entry_set | length),
          set_class: set_class($entry_set)
        },
        relationship:
          (if ($unit_set | length) == 1
              and ($entry_set | length) == 1
              and $unit_ids[0] == $entry_ids[0]
           then "same_unique_offer"
           elif $overlap > 0 then "overlapping_tie_sets"
           else "disjoint_offer_sets"
           end)
      }
  end;

def state($records_seen; $comparable; $contradictions):
  if $records_seen == 0 then
    {status: "no_current_asks", reason: "valid_empty_list"}
  elif $contradictions > 0 then
    {status: "contradicted", reason: "positive_complete_tuple_violates_ratio"}
  elif $comparable > 0 then
    {status: "supported", reason: "comparable_tuples_without_contradiction"}
  else
    {status: "inconclusive", reason: "nonempty_without_comparable_tuple"}
  end;

if ($response | length) != 1 then
  fail("exactly one response is required")
elif ($canonical_id | IN("base:r01", "base:r05", "base:r10", "base:r17", "base:r23", "base:r33") | not) then
  fail("unexpected canonical rune ID")
elif ($response[0] | type) != "object"
  or (($response[0].StatusCode // null) | zero_code | not)
  or ($response[0].StatusData | type) != "object"
  or (($response[0].StatusData.ResultCode // null) | zero_code | not)
  or ($response[0].StatusData.ResultData | type) != "array"
  or any($response[0].StatusData.ResultData[];
      type != "object" or (.title | type) != "string") then
  fail("invalid listing response shape")
else
  $response[0].StatusData.ResultData as $records
  | [$records[]
     | if privacy_bearing then {status: "privacy"}
       else {status: "eligible", record: .}
       end] as $classified
  | [$classified[] | select(.status == "eligible") | .record] as $identity_eligible
  | reduce $identity_eligible[] as $record
      ({seen: [], kept: [], duplicates: 0};
       ($record.shopno // null) as $shopno
       | if (($shopno | type) | IN("string", "number") | not)
            or (($shopno | tostring) == "") then
           fail("eligible record has invalid listing identity")
         elif (.seen | index($shopno | tostring)) != null then
           .duplicates += 1
         else
           .seen += [($shopno | tostring)]
           | .kept += [$record]
         end) as $deduplicated
  | $deduplicated.kept as $rows
  | [$rows[] | tuple(.; $relative_tolerance)] as $tuple_rows
  | [$tuple_rows[] | select(.relation == "matches")] as $comparable
  | ([$tuple_rows[] | select(.relation == "contradicts")] | length) as $contradictions
  | {
      canonical_id: $canonical_id,
      counts: {
        records_seen: ($records | length),
        privacy_excluded: ([$classified[] | select(.status == "privacy")] | length),
        duplicate_excluded: $deduplicated.duplicates,
        records_classified: ($rows | length),
        fields: {
          price: field_counts($rows; "price"; false),
          singleprice: field_counts($rows; "singleprice"; false),
          amount: field_counts($rows; "amount"; true),
          number: field_counts($rows; "number"; true)
        },
        comparable_tuples: ($comparable | length),
        amount_bins: {
          one: ([$comparable[] | select(quantity_bin(.amount) == "one")] | length),
          two_to_nine: ([$comparable[] | select(quantity_bin(.amount) == "two_to_nine")] | length),
          ten_to_ninety_nine: ([$comparable[] | select(quantity_bin(.amount) == "ten_to_ninety_nine")] | length),
          hundred_or_more: ([$comparable[] | select(quantity_bin(.amount) == "hundred_or_more")] | length)
        },
        ratio_relations: {
          matches: ($comparable | length),
          contradicts: $contradictions,
          insufficient: ([$tuple_rows[] | select(.relation == "insufficient")] | length)
        }
      },
      minimum_sets: minima($comparable; $relative_tolerance),
      state: state(($records | length); ($comparable | length); $contradictions)
    }
end
