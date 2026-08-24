#!/usr/bin/env bash
set -euo pipefail

readonly SUMMARIZER='research/dd373/summarize-rune-lot-semantics.jq'
readonly FIXTURE='research/dd373/fixtures/rune-lot-semantics-api-response.json'
readonly EXPECTED='research/dd373/fixtures/rune-lot-semantics-expected-report.json'
readonly REPORT='research/dd373/rune-lot-semantics-report.json'
readonly MANIFEST='research/dd373/rune-lot-semantics-manifest.json'
readonly TOLERANCE='0.000001'
readonly GAME_LABEL='暗黑2：重制版国服'
readonly AREA_LABEL='非赛季'
readonly SERVER_LABEL='非赛季(术士君临)'
readonly USER_AGENT='Arreat-Index-Rune-Lot-Semantics-Research/1.0'
readonly -a FIXED_RUNES=(01 05 10 17 23 33)

validate_manifest_pacing() {
  jq -e '
    .request_count == 11
    and (.requests | length) == 11
    and .minimum_start_interval_ms == 1100
    and .raw_responses_retained == false
    and ((.requests | map(.started_at_ms)) as $starts
         | all(range(1; ($starts | length));
               . as $i | ($starts[$i] - $starts[$i - 1] >= 1100)))
  ' "$1" >/dev/null
}

verify_pacing() {
  local tmp valid invalid
  tmp=$(mktemp -d)
  trap "find '$tmp' -depth -delete" EXIT
  valid="$tmp/valid.json"
  invalid="$tmp/invalid.json"
  jq -n '{request_count:11,minimum_start_interval_ms:1100,raw_responses_retained:false,
    requests:[range(0;11) | {started_at_ms:(1700000000000 + (. * 1100))}]}' > "$valid"
  validate_manifest_pacing "$valid"
  jq '(.requests[7].started_at_ms) -= 1' "$valid" > "$invalid"
  if validate_manifest_pacing "$invalid"; then
    echo '1099 ms pacing interval was accepted' >&2
    exit 1
  fi
}

validate_retained() {
  jq -e '
    def forbidden_key:
      test("^(title|shopno|listing_?id|seller|contact|account|memo|description|detail_?url|raw_?body|response|record|row|price_cny|amount_value|number_value|singleprice_value|identity|raw)$"; "i");
    def provider_value_key:
      ascii_downcase
      | IN("price", "singleprice", "amount", "number", "lot_price", "entry_price",
           "unit_price", "quantity_per_lot", "available_lots");
    def provider_numeric_value:
      type == "number"
      or (type == "string" and test("^-?[0-9]+(?:\\.[0-9]+)?$"));
    ([.. | objects | keys[] | select(forbidden_key)] | length) == 0
    and ([.. | objects | to_entries[]
      | select((.key | provider_value_key) and (.value | provider_numeric_value))]
      | length) == 0
    and ([.. | strings
      | select(test("synthetic rune|synthetic duplicate|contact qq|not-a-decimal|private"; "i"))]
      | length) == 0
  ' "$1" >/dev/null
}

validate_selected_game() {
  local file=$1 label=$2 id=$3
  jq -e --arg label "$label" --arg id "$id" '
    def matching_row:
      ((.Name? == $label) or (.name? == $label))
      and ((.Id? == $id) or (.id? == $id));
    [.. | objects | select(matching_row)] as $matches
    | ($matches | length) == 1
      and ($matches[0]
        | (.IsClose | type) == "boolean" and .IsClose == false
        and (.CanTrade | type) == "boolean" and .CanTrade == true
        and (.IsEnabled | type) == "boolean" and .IsEnabled == true
        and ((has("isClose") | not) or .isClose == .IsClose)
        and ((has("canTrade") | not) or .canTrade == .CanTrade)
        and ((has("isEnabled") | not) or .isEnabled == .IsEnabled))
  ' "$file" >/dev/null
}

verify_selected_game() {
  local label='fixture game' id='fixture-game-id' valid invalid_name invalid
  valid='{"Name":"fixture game","Id":"fixture-game-id","IsClose":false,"CanTrade":true,"IsEnabled":true}'
  printf '%s\n' "$valid" | validate_selected_game /dev/stdin "$label" "$id"
  while IFS=$'\t' read -r invalid_name invalid; do
    if printf '%s\n' "$invalid" | validate_selected_game /dev/stdin "$label" "$id"; then
      echo "invalid selected-game state was accepted: $invalid_name" >&2
      return 1
    fi
  done <<'EOF'
closed	{"Name":"fixture game","Id":"fixture-game-id","IsClose":true,"CanTrade":true,"IsEnabled":true}
untradeable	{"Name":"fixture game","Id":"fixture-game-id","IsClose":false,"CanTrade":false,"IsEnabled":true}
disabled	{"Name":"fixture game","Id":"fixture-game-id","IsClose":false,"CanTrade":true,"IsEnabled":false}
missing flag	{"Name":"fixture game","Id":"fixture-game-id","IsClose":false,"CanTrade":true}
conflicting flag	{"Name":"fixture game","Id":"fixture-game-id","IsClose":false,"CanTrade":true,"IsEnabled":true,"isEnabled":false}
EOF
}

validate_report_shape() {
  jq -e '
    .report_version == 1
    and (.evidence_kind | IN("synthetic_fixture", "live_capture"))
    and .scope == {game:"暗黑2：重制版国服",season:"非赛季",play_mode:"normal",server:"非赛季(术士君临)"}
    and .fixed_canonical_ids == ["base:r01","base:r05","base:r10","base:r17","base:r23","base:r33"]
    and (.community_sources | length) == 3
    and ([.community_sources[].commit] | sort) == ([
      "cebecdf5a340a4fc00132bca663f8b263041ac9c",
      "60bb917729acee194485ed81d16048cadd0c4aef",
      "108e0f98c68ec671b6c108ff6492b698284d72f2"] | sort)
    and (.runes | length) == 6
    and ([.runes[].canonical_id] | sort) == (["base:r01","base:r05","base:r10","base:r17","base:r23","base:r33"] | sort)
    and all(.runes[];
      (.counts.records_seen | numbers) and . >= 0
      and (.counts.privacy_excluded | numbers) and . >= 0
      and (.counts.duplicate_excluded | numbers) and . >= 0
      and (.counts.comparable_tuples | numbers) and . >= 0
      and (.state.status | IN("no_current_asks","supported","contradicted","inconclusive"))
      and (.minimum_sets.relationship | IN("same_unique_offer","overlapping_tie_sets","disjoint_offer_sets","not_comparable"))
      and (.minimum_sets.unit_minimum.set_class | IN("single_only","grouped_only","mixed","not_comparable"))
      and (.minimum_sets.entry_minimum.set_class | IN("single_only","grouped_only","mixed","not_comparable")))
    and (.global.field_model_status | IN("supported","contradicted","inconclusive"))
    and (.global.grouped_offers_observed | type) == "boolean"
    and (.global.ranking_divergence_observed | type) == "boolean"
  ' "$1" >/dev/null
  validate_retained "$1"
}

summarize_one() {
  local canonical_id=$1 response=$2 output=$3
  jq -S -n --arg canonical_id "$canonical_id" \
    --argjson relative_tolerance "$TOLERANCE" --slurpfile response "$response" \
    -f "$SUMMARIZER" > "$output"
  validate_retained "$output"
}

build_report() {
  local evidence_kind=$1 captured_at=$2 segments=$3 output=$4
  jq -S -n --arg evidence_kind "$evidence_kind" --arg captured_at "$captured_at" \
    --argjson relative_tolerance "$TOLERANCE" --slurpfile runes "$segments" '
    ($runes | sort_by(.canonical_id)) as $ordered
    | ([$ordered[] | select(.state.status == "supported")] | length) as $supported
    | ([$ordered[] | select(.state.status == "contradicted")] | length) as $contradicted
    | ([$ordered[] | select(.counts.amount_bins.two_to_nine
                            + .counts.amount_bins.ten_to_ninety_nine
                            + .counts.amount_bins.hundred_or_more > 0)] | length) as $grouped_pages
    | ([$ordered[] | select(.minimum_sets.relationship == "disjoint_offer_sets")] | length) as $disjoint_pages
    | {
        report_version: 1,
        evidence_kind: $evidence_kind,
        captured_at: (if $captured_at == "" then null else $captured_at end),
        scope: {game:"暗黑2：重制版国服",season:"非赛季",play_mode:"normal",server:"非赛季(术士君临)"},
        fixed_canonical_ids: ["base:r01","base:r05","base:r10","base:r17","base:r23","base:r33"],
        method: {
          relative_tolerance: $relative_tolerance,
          tuple_fields: {amount:"quantity_per_lot",price:"entry_price",number:"available_lots",singleprice:"unit_price"},
          amount_and_number_are_fallback_aliases: false,
          quantity_bins: ["one","two_to_nine","ten_to_ninety_nine","hundred_or_more"],
          filter_order: ["privacy","duplicate","field_classification","tuple_relation","minimum_sets"]
        },
        official_source: {
          url:"https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html",
          evidence_origin:"retained_plan018_inspection",
          semantic_summary:"For ordinary ratio goods, number is published lot count, amount is quantity in one lot, singleprice is the unit-price ratio, and price is product price."
        },
        community_sources: [
          {
            repository:"HuskyCommunicator/d2r-price-qq-bot",
            commit:"cebecdf5a340a4fc00132bca663f8b263041ac9c",
            inspected_path:"skill/scripts/fetch_373.py",inspected_range:"563-677",detected_license:"none_detected",
            role:"Distinguishes equipment display price from rune per-unit expressions and summarizes runes by unit price; it omits minimum group quantity and Entry Price."
          },
          {
            repository:"lhe6330-cloud/d2r-equipment-checker",
            commit:"60bb917729acee194485ed81d16048cadd0c4aef",
            inspected_path:"crawler.py",inspected_range:"242-277",detected_license:"none_detected",
            role:"Supports fixed-name equipment display-price handling but contains no rune-lot model."
          },
          {
            repository:"SirYuxuan/astrbot-plugin-dnf",
            commit:"108e0f98c68ec671b6c108ff6492b698284d72f2",
            inspected_path:"dnf_plugin/dnf_utils.py",inspected_range:"40-76",detected_license:"AGPL-3.0",
            role:"Uses the ordinary-goods endpoint and derives a comparison ratio; its amount-or-number fallback conflates quantity per lot with available lot count and is explicitly rejected here."
          }
        ],
        runes: $ordered,
        global: {
          field_model_status:
            (if $contradicted > 0 then "contradicted"
             elif $supported > 0 then "supported"
             else "inconclusive" end),
          grouped_offers_observed: ($grouped_pages > 0),
          rune_pages_with_grouped_offers: $grouped_pages,
          ranking_divergence_observed: ($disjoint_pages > 0),
          comparable_pages_with_disjoint_minima: $disjoint_pages
        },
        later_model_boundary:"This aggregate proof does not change production aggregation, select a public schema, or combine independently minimized fields into an offer."
      }
  ' > "$output"
  validate_report_shape "$output"
}

fixture_mode() {
  local tmp segments sample response result canonical invalid retained_name retained_json
  tmp=$(mktemp -d)
  trap "find '$tmp' -depth -delete" EXIT
  segments="$tmp/segments.ndjson"
  : > "$segments"
  for index in 0 1 2 3 4 5; do
    sample="$tmp/sample-$index.json"
    response="$tmp/response-$index.json"
    result="$tmp/result-$index.json"
    jq -S ".samples[$index]" "$FIXTURE" > "$sample"
    canonical=$(jq -er .canonical_id "$sample")
    jq -S '.response' "$sample" > "$response"
    summarize_one "$canonical" "$response" "$result"
    jq -c . "$result" >> "$segments"
  done
  while IFS= read -r index; do
    invalid="$tmp/invalid-$index.json"
    jq -S ".invalid_response_shapes[$index]" "$FIXTURE" > "$invalid"
    if summarize_one 'base:r01' "$invalid" "$tmp/should-fail.json" 2>/dev/null; then
      echo "invalid response shape $index was accepted" >&2
      exit 1
    fi
  done < <(jq -r '.invalid_response_shapes | keys[]' "$FIXTURE")
  printf '%s\n' '{"title":"must fail"}' > "$tmp/forbidden.json"
  if validate_retained "$tmp/forbidden.json"; then
    echo 'forbidden retained key was accepted' >&2
    exit 1
  fi
  while IFS=$'\t' read -r retained_name retained_json; do
    printf '%s\n' "$retained_json" > "$tmp/retained-negative.json"
    if validate_retained "$tmp/retained-negative.json"; then
      echo "forbidden retained data was accepted: $retained_name" >&2
      exit 1
    fi
  done <<'EOF'
identity	{"identity":"shop-123"}
raw	{"raw":"secret"}
numeric price	{"price":42}
numeric-string amount	{"amount":"5"}
EOF
  verify_selected_game
  build_report synthetic_fixture '' "$segments" "$tmp/report.json"
  jq -e '
    (.runes[] | select(.canonical_id == "base:r17")) as $r17
    | $r17.counts.comparable_tuples == 0
      and $r17.counts.fields.amount.positive_integral > 0
      and $r17.counts.fields.number.positive_integral > 0
      and $r17.state.status == "inconclusive"
  ' "$tmp/report.json" >/dev/null || {
    echo 'amount/number field-separation regression failed' >&2
    exit 1
  }
  jq -S . "$tmp/report.json"
}

offline_gates() {
  local tmp
  bash -n "$0"
  jq -n --arg canonical_id base:r01 --argjson relative_tolerance "$TOLERANCE" \
    --argjson response '[{"StatusCode":0,"StatusData":{"ResultCode":0,"ResultData":[]}}]' \
    -f "$SUMMARIZER" >/dev/null
  tmp=$(mktemp)
  trap "rm -f '$tmp'" RETURN
  "$0" --fixture-mode > "$tmp"
  cmp -s "$tmp" "$EXPECTED" || { echo 'fixture output differs from canonical expected report' >&2; return 1; }
  "$0" --verify-pacing
  [[ $(rg -n -F "curl --disable --noproxy '*'" "$0" | rg -v 'rg -n -F' | wc -l) -eq 1 ]] || {
    echo "every live curl call must contain literal --noproxy '*'" >&2
    return 1
  }
  trap - RETURN
  rm -f "$tmp"
}

if [[ "${1:-}" == '--fixture-mode' ]]; then
  [[ $# == 1 ]] || { echo 'fixture mode accepts no other arguments' >&2; exit 2; }
  fixture_mode
  exit 0
elif [[ "${1:-}" == '--verify-pacing' ]]; then
  [[ $# == 1 ]] || { echo 'pacing verification accepts no other arguments' >&2; exit 2; }
  verify_pacing
  exit 0
elif [[ "${1:-}" == '--validate-retained' ]]; then
  [[ $# == 2 ]] || { echo 'usage: --validate-retained FILE' >&2; exit 2; }
  validate_retained "$2"
  exit 0
elif [[ "${1:-}" == '--live' ]]; then
  [[ $# == 1 ]] || { echo 'live mode accepts no other arguments' >&2; exit 2; }
else
  echo 'usage: capture-rune-lot-semantics.sh [--fixture-mode|--verify-pacing|--validate-retained FILE|--live]' >&2
  exit 2
fi

[[ -n "${IMPROVE_EXECUTION_ID:-}" ]] || { echo 'IMPROVE_EXECUTION_ID must be nonempty' >&2; exit 1; }
readonly CACHE_ROOT=".cache/arreat-index/plan022-$IMPROVE_EXECUTION_ID"
readonly RAW="$CACHE_ROOT/raw"
readonly LEDGER="$CACHE_ROOT/request-ledger.ndjson"
readonly SEGMENTS="$CACHE_ROOT/rune-segments.ndjson"
[[ ! -e "$CACHE_ROOT" ]] || { echo 'live capture cache coordinate already exists' >&2; exit 1; }
[[ ! -e "$REPORT" && ! -e "$MANIFEST" ]] || { echo 'retained output target already exists' >&2; exit 1; }

offline_gates
mkdir -p "$RAW"
: > "$LEDGER"
: > "$SEGMENTS"

cleanup_coordinate() {
  if [[ -d "$CACHE_ROOT" ]]; then find "$CACHE_ROOT" -depth -delete; fi
}
trap cleanup_coordinate EXIT

request_count=0
last_start_ms=0
request() {
  local purpose=$1 url=$2 output=$3 host now wait_ms status content_type bytes digest headers
  [[ "$url" =~ ^https://([^/]+)/ ]] || { echo 'non-HTTPS URL rejected' >&2; exit 1; }
  host=${BASH_REMATCH[1]}
  [[ "$host" == 'game.dd373.com' || "$host" == 'goods.dd373.com' ]] || {
    echo "host rejected: $host" >&2
    exit 1
  }
  (( request_count += 1 ))
  (( request_count <= 11 )) || { echo 'request budget exceeded' >&2; exit 1; }
  now=$(date +%s%3N)
  if (( last_start_ms > 0 && now - last_start_ms < 1100 )); then
    wait_ms=$((1100 - (now - last_start_ms)))
    sleep "$(awk -v ms="$wait_ms" 'BEGIN {printf "%.3f",ms/1000}')"
    now=$(date +%s%3N)
  fi
  (( last_start_ms == 0 || now - last_start_ms >= 1100 )) || {
    echo 'request pacing violation' >&2
    exit 1
  }
  last_start_ms=$now
  headers="$output.headers"
  status=$(curl --disable --noproxy '*' --silent --show-error --proto '=https' --max-redirs 0 \
    --connect-timeout 20 --max-time 60 --user-agent "$USER_AGENT" --output "$output" \
    --dump-header "$headers" --write-out '%{http_code}' "$url") || {
      echo "network request failed without retry: $url" >&2
      exit 1
    }
  [[ "$status" == 200 ]] || { echo "HTTP $status without retry: $url" >&2; exit 1; }
  content_type=$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,"");sub(/\r$/,"");v=$0} END{print v}' "$headers")
  [[ "$content_type" == application/json* ]] || {
    echo "unexpected JSON content type: $content_type" >&2
    exit 1
  }
  [[ -s "$output" ]] || { echo "empty response: $url" >&2; exit 1; }
  jq -e . "$output" >/dev/null || { echo "invalid JSON response: $url" >&2; exit 1; }
  if LC_ALL=C grep -Eiq 'captcha|访问验证|安全验证|登录后|请登录|browser challenge|cloudflare ray' "$output"; then
    echo "login/cookie/challenge marker detected: $url" >&2
    exit 1
  fi
  bytes=$(wc -c < "$output" | tr -d ' ')
  digest=$(sha256sum "$output" | cut -d' ' -f1)
  jq -cn --arg purpose "$purpose" --arg url "$url" --argjson started_at_ms "$now" \
    --argjson http_status "$status" --arg content_type "$content_type" --argjson bytes "$bytes" \
    --arg sha256 "$digest" \
    '{purpose:$purpose,url:$url,started_at_ms:$started_at_ms,http_status:$http_status,content_type:$content_type,bytes:$bytes,sha256:$sha256}' \
    >> "$LEDGER"
  rm -f "$headers"
}

taxonomy_rows() {
  jq -ce '
    [.. | objects | select(has("Name") or has("name") or has("Id") or has("id"))
      | (if has("Name") and has("name") then
           if (.Name|type)=="string" and (.name|type)=="string" and .Name==.name then .Name
           else error("incomplete or inconsistent taxonomy name") end
         elif has("Name") and (.Name|type)=="string" then .Name
         elif has("name") and (.name|type)=="string" then .name
         else error("incomplete taxonomy name") end) as $name
      | (if has("Id") and has("id") then
           if (.Id|type)=="string" and (.id|type)=="string" and .Id==.id then .Id
           else error("incomplete or inconsistent taxonomy ID") end
         elif has("Id") and (.Id|type)=="string" then .Id
         elif has("id") and (.id|type)=="string" then .id
         else error("incomplete taxonomy ID") end) as $id
      | if ($id | test("^[A-Za-z0-9-]+$") | not) or $name == "" then error("unsafe taxonomy row")
        else {name:$name,id:$id}
        end] as $rows
    | if ($rows | length) == 0 then error("taxonomy contains no rows")
      elif ($rows | unique_by(.name) | length) != ($rows | length) then error("duplicate taxonomy name")
      elif ($rows | unique_by(.id) | length) != ($rows | length) then error("duplicate taxonomy ID")
      else $rows
      end
  ' "$1"
}

exact_id() {
  local file=$1 label=$2
  taxonomy_rows "$file" | jq -er --arg label "$label" \
    '[.[] | select(.name == $label) | .id] | if length == 1 then .[0] else error("taxonomy exact-match failure: " + $label) end'
}

game="$RAW/01-game.json"
areas="$RAW/02-areas.json"
servers="$RAW/03-servers.json"
roots="$RAW/04-roots.json"
runes="$RAW/05-runes.json"
request game_list 'https://game.dd373.com/api/game/list' "$game"
game_id=$(exact_id "$game" "$GAME_LABEL")
validate_selected_game "$game" "$GAME_LABEL" "$game_id" || {
  echo 'exact game is not uniquely enabled and tradeable' >&2
  exit 1
}
request areas "https://game.dd373.com/Api/GameOther/List?parentId=$game_id" "$areas"
area_id=$(exact_id "$areas" "$AREA_LABEL")
request servers "https://game.dd373.com/Api/GameOther/List?parentId=$area_id" "$servers"
server_id=$(exact_id "$servers" "$SERVER_LABEL")
request goods_roots "https://game.dd373.com/Api/GameGoodsType/List?parentId=$game_id" "$roots"
rune_root=$(exact_id "$roots" '符文')
request rune_children "https://game.dd373.com/Api/GameGoodsType/List?parentId=$rune_root" "$runes"

rune_map="$CACHE_ROOT/rune-map.json"
taxonomy_rows "$runes" > "$CACHE_ROOT/rune-rows.json"
jq -e '
  reduce range(1;34) as $n ({};
    ([$rows[0][] | select(.name == (($n|tostring) + "号符文")) | .id]) as $matches
    | if ($matches | length) != 1 then error("missing or ambiguous rune leaf: " + ($n|tostring))
      else .[("base:r" + (if $n < 10 then "0" else "" end) + ($n|tostring))] = $matches[0]
      end)
' --slurpfile rows "$CACHE_ROOT/rune-rows.json" -n > "$rune_map"
[[ $(jq 'length' "$rune_map") -eq 33 ]] || { echo 'complete 1..33 rune taxonomy not proven' >&2; exit 1; }

realm_path="${area_id}_${server_id}"
for rune in "${FIXED_RUNES[@]}"; do
  canonical="base:r$rune"
  leaf=$(jq -er --arg canonical "$canonical" '.[$canonical]' "$rune_map")
  raw="$RAW/listing-$rune.json"
  result="$CACHE_ROOT/segment-$rune.json"
  url="https://goods.dd373.com/Api/Goods/UserCenter/ApiGetShopList?gameid=$game_id&GameOtherId=$realm_path&GameShopTypeId=$leaf"
  request "rune_${rune}_listing" "$url" "$raw"
  summarize_one "$canonical" "$raw" "$result"
  jq -c . "$result" >> "$SEGMENTS"
  rm -f "$raw" "$result"
done

(( request_count == 11 )) || { echo "expected exactly 11 requests, observed $request_count" >&2; exit 1; }
[[ $(wc -l < "$LEDGER") -eq 11 ]] || { echo 'request ledger count mismatch' >&2; exit 1; }

captured_at=$(date -u +%FT%TZ)
build_report live_capture "$captured_at" "$SEGMENTS" "$CACHE_ROOT/report.json.tmp"
jq -S -s --arg captured_at "$captured_at" --arg user_agent "$USER_AGENT" \
  --arg game "$GAME_LABEL" --arg area "$AREA_LABEL" --arg server "$SERVER_LABEL" '
  {
    manifest_version:1,captured_at:$captured_at,user_agent:$user_agent,
    request_count:length,minimum_start_interval_ms:1100,
    scope:{game:$game,season:$area,play_mode:"normal",server:$server},
    requests:.,raw_responses_retained:false
  }
' "$LEDGER" > "$CACHE_ROOT/manifest.json.tmp"
validate_report_shape "$CACHE_ROOT/report.json.tmp"
validate_retained "$CACHE_ROOT/manifest.json.tmp"
validate_manifest_pacing "$CACHE_ROOT/manifest.json.tmp"
mv "$CACHE_ROOT/report.json.tmp" "$REPORT"
mv "$CACHE_ROOT/manifest.json.tmp" "$MANIFEST"
validate_report_shape "$REPORT"
validate_retained "$MANIFEST"
validate_manifest_pacing "$MANIFEST"
cleanup_coordinate
trap - EXIT
[[ ! -e "$CACHE_ROOT" ]] || { echo 'temporary coordinate cleanup failed' >&2; exit 1; }
