#!/usr/bin/env bash
set -euo pipefail

readonly SUMMARIZER='research/dd373/summarize-price-semantics.jq'
readonly FIXTURE='research/dd373/fixtures/price-semantics-api-response.json'
readonly EXPECTED='research/dd373/fixtures/price-semantics-expected-report.json'
readonly REPORT='research/dd373/price-semantics-report.json'
readonly MANIFEST='research/dd373/price-semantics-manifest.json'
readonly TOLERANCE='0.000001'
readonly GAME_LABEL='暗黑2：重制版国服'
readonly AREA_LABEL='非赛季'
readonly SERVER_LABEL='非赛季(术士君临)'
readonly USER_AGENT='Arreat-Index-Price-Semantics-Research/1.0'

validate_manifest_pacing() {
  jq -e '
    .request_count == 13
    and (.requests | length) == 13
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
  jq -n '{request_count:13,raw_responses_retained:false,
    requests:[range(0;13) | {started_at_ms:(1700000000000 + (. * 1100))}]}' > "$valid"
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
      test("^(title|shopno|listing_id|seller|contact|account|raw_body|response|record|detail_url|price_cny|amount_value|unit_value)$"; "i");
    ([.. | objects | keys[] | select(forbidden_key)] | length) == 0
    and ([.. | objects | select(has("title") or has("shopno"))] | length) == 0
    and ([.. | strings | select(test("r-forward|u-empty|s-empty|not-a-decimal|synthetic duplicate"; "i"))] | length) == 0
  ' "$1" >/dev/null
}

summarize_one() {
  local family=$1 canonical_id=$2 catalog=$3 response=$4 output=$5
  jq -S -n --arg family "$family" --arg canonical_id "$canonical_id" \
    --argjson relative_tolerance "$TOLERANCE" --slurpfile catalog "$catalog" \
    --slurpfile response "$response" -f "$SUMMARIZER" > "$output"
  validate_retained "$output"
}

build_report() {
  local evidence_kind=$1 captured_at=$2 segments=$3 output=$4
  jq -S -n --arg evidence_kind "$evidence_kind" --arg captured_at "$captured_at" \
    --argjson relative_tolerance "$TOLERANCE" --slurpfile families "$segments" '
    ($families | sort_by(.family)) as $ordered
    | ([$ordered[].conclusion.status]) as $statuses
    | {
        report_version: 1,
        evidence_kind: $evidence_kind,
        captured_at: (if $captured_at == "" then null else $captured_at end),
        scope: {game:"暗黑2：重制版国服",season:"非赛季",play_mode:"normal",server:"非赛季(术士君临)"},
        fixed_canonical_ids: ["base:r17","unique:The Oculus","set-item:Tal Rasha\u0027s Adjudication"],
        method: {
          relative_tolerance: $relative_tolerance,
          relation_classes: [
            "price_over_amount_matches_singleprice",
            "amount_over_price_matches_singleprice",
            "unit_quantity_ambiguous",
            "neither_or_insufficient"
          ],
          filter_order: ["privacy","multi_item","unmatched","duplicate","field_classification"]
        },
        community_sources: [
          {repository:"HuskyCommunicator/d2r-price-qq-bot",commit:"cebecdf5a340a4fc00132bca663f8b263041ac9c",inspected_path:"skill/scripts/fetch_373.py",inspected_range:"563-677",detected_license:"none_detected",behavior_summary:"Parses the displayed equipment card price and uses a displayed per-item expression only when present."},
          {repository:"lhe6330-cloud/d2r-equipment-checker",commit:"60bb917729acee194485ed81d16048cadd0c4aef",inspected_path:"crawler.py",inspected_range:"242-277",detected_license:"none_detected",behavior_summary:"Parses the displayed equipment card price."},
          {repository:"SirYuxuan/astrbot-plugin-dnf",commit:"108e0f98c68ec671b6c108ff6492b698284d72f2",inspected_path:"dnf_plugin/dnf_utils.py",inspected_range:"40-76",detected_license:"AGPL-3.0",behavior_summary:"Derives a ratio from quantity and product price without using the unit label to choose the monetary field."}
        ],
        official_sources: [
          {url:"https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html",semantic_summary:"Defines singleprice as a unit-price ratio, amount as per-item game-currency quantity, and price as product price for ratio goods."},
          {url:"https://kf.dd373.com/helpdetail/1f090ff7dea949108a23e3ac4a771c08.html",semantic_summary:"Separately names item price, game quantity, game-currency unit price, and order total."}
        ],
        families: $ordered,
        global_conclusion: {
          status: (if ($statuses | index("contradicted")) != null then "contradicted"
                   elif ($statuses | all(. == "supported")) then "supported"
                   else "inconclusive" end),
          reason: (if ($statuses | index("contradicted")) != null then "at_least_one_family_contradicted"
                   elif ($statuses | all(. == "supported")) then "all_fixed_families_supported"
                   else "at_least_one_family_inconclusive" end),
          later_model_boundary:"A later plan may use only these family conclusions and aggregate counts; this report selects no production price basis, public field name, or schema version."
        }
      }
  ' > "$output"
  validate_retained "$output"
}

fixture_mode() {
  local tmp catalog segments sample family canonical response result invalid
  tmp=$(mktemp -d)
  trap "find '$tmp' -depth -delete" EXIT
  catalog="$tmp/catalog.json"
  segments="$tmp/segments.json"
  jq -S '.catalog' "$FIXTURE" > "$catalog"
  : > "$segments"
  for index in 0 1 2; do
    sample="$tmp/sample-$index.json"
    response="$tmp/response-$index.json"
    result="$tmp/result-$index.json"
    jq -S ".samples[$index]" "$FIXTURE" > "$sample"
    family=$(jq -er .family "$sample")
    canonical=$(jq -er .canonical_id "$sample")
    jq -S '.response' "$sample" > "$response"
    summarize_one "$family" "$canonical" "$catalog" "$response" "$result"
    jq -c . "$result" >> "$segments"
  done
  while IFS= read -r index; do
    invalid="$tmp/invalid-$index.json"
    jq -S ".invalid_response_shapes[$index]" "$FIXTURE" > "$invalid"
    if summarize_one rune 'base:r17' "$catalog" "$invalid" "$tmp/should-fail.json" 2>/dev/null; then
      echo "invalid response shape $index was accepted" >&2
      exit 1
    fi
  done < <(jq -r '.invalid_response_shapes | keys[]' "$FIXTURE")
  jq '.candidate_groups.unique[0].source="unapproved"' "$catalog" > "$tmp/bad-catalog.json"
  if summarize_one unique 'unique:The Oculus' "$tmp/bad-catalog.json" "$tmp/response-1.json" "$tmp/should-fail.json" 2>/dev/null; then
    echo 'malformed catalog candidate was accepted' >&2
    exit 1
  fi
  printf '%s\n' '{"title":"must fail"}' > "$tmp/forbidden.json"
  if validate_retained "$tmp/forbidden.json"; then
    echo 'forbidden retained key was accepted' >&2
    exit 1
  fi
  printf '%s\n' '{"safe":{"record":{"x":1}}}' > "$tmp/record.json"
  if validate_retained "$tmp/record.json"; then
    echo 'retained record object was accepted' >&2
    exit 1
  fi
  build_report synthetic_fixture '' "$segments" "$tmp/report.json"
  jq -S . "$tmp/report.json"
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
elif [[ $# != 0 ]]; then
  echo 'usage: capture-price-semantics.sh [--fixture-mode|--verify-pacing|--validate-retained FILE]' >&2
  exit 2
fi

[[ -n "${IMPROVE_EXECUTION_ID:-}" ]] || { echo 'IMPROVE_EXECUTION_ID must be nonempty' >&2; exit 1; }
readonly CACHE_ROOT=".cache/arreat-index/plan018-$IMPROVE_EXECUTION_ID"
readonly CATALOG="$CACHE_ROOT/catalog.json"
readonly RAW="$CACHE_ROOT/raw"
readonly LEDGER="$CACHE_ROOT/request-ledger.ndjson"
readonly SEGMENTS="$CACHE_ROOT/family-segments.ndjson"
[[ -d "$CACHE_ROOT" && -f "$CATALOG" ]] || { echo 'prepared execution catalog is missing' >&2; exit 1; }
[[ ! -e "$REPORT" && ! -e "$MANIFEST" ]] || { echo 'retained output target already exists' >&2; exit 1; }
[[ ! -e "$RAW" && ! -e "$LEDGER" && ! -e "$SEGMENTS" ]] || { echo 'live capture coordinate is not clean' >&2; exit 1; }
mkdir "$RAW"
: > "$LEDGER"
: > "$SEGMENTS"

cleanup_coordinate() {
  if [[ -d "$CACHE_ROOT" ]]; then find "$CACHE_ROOT" -depth -delete; fi
}
trap cleanup_coordinate EXIT

request_count=0
last_start_ms=0
request() {
  local purpose=$1 kind=$2 url=$3 output=$4 host now wait_ms status content_type bytes digest headers
  [[ "$url" =~ ^https://([^/]+)/ ]] || { echo 'non-HTTPS URL rejected' >&2; exit 1; }
  host=${BASH_REMATCH[1]}
  [[ "$host" == 'kf.dd373.com' || "$host" == 'about.dd373.com' || "$host" == 'game.dd373.com' || "$host" == 'goods.dd373.com' ]] || { echo "host rejected: $host" >&2; exit 1; }
  (( request_count += 1 ))
  (( request_count <= 13 )) || { echo 'request budget exceeded' >&2; exit 1; }
  now=$(date +%s%3N)
  if (( last_start_ms > 0 && now - last_start_ms < 1100 )); then
    wait_ms=$((1100 - (now - last_start_ms)))
    sleep "$(awk -v ms="$wait_ms" 'BEGIN {printf "%.3f",ms/1000}')"
    now=$(date +%s%3N)
  fi
  (( last_start_ms == 0 || now - last_start_ms >= 1100 )) || { echo 'request pacing violation' >&2; exit 1; }
  last_start_ms=$now
  headers="$output.headers"
  status=$(curl --silent --show-error --proto '=https' --max-redirs 0 --connect-timeout 20 --max-time 60 \
    --user-agent "$USER_AGENT" --output "$output" --dump-header "$headers" --write-out '%{http_code}' "$url") \
    || { echo "network request failed without retry: $url" >&2; exit 1; }
  [[ "$status" == 200 ]] || { echo "HTTP $status without retry: $url" >&2; exit 1; }
  content_type=$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,"");sub(/\r$/,"");v=$0} END{print v}' "$headers")
  [[ -s "$output" ]] || { echo "empty response: $url" >&2; exit 1; }
  if [[ "$kind" == json ]]; then
    [[ "$content_type" == application/json* ]] || { echo "unexpected JSON content type: $content_type" >&2; exit 1; }
    jq -e . "$output" >/dev/null || { echo "invalid JSON response: $url" >&2; exit 1; }
  else
    [[ "$content_type" == text/html* ]] || { echo "unexpected HTML content type: $content_type" >&2; exit 1; }
  fi
  if LC_ALL=C grep -Eiq 'captcha|访问验证|安全验证|登录后|请登录|browser challenge|cloudflare ray' "$output"; then
    echo "login/cookie/challenge marker detected: $url" >&2; exit 1
  fi
  bytes=$(wc -c < "$output" | tr -d ' ')
  digest=$(sha256sum "$output" | cut -d' ' -f1)
  jq -cn --arg purpose "$purpose" --arg url "$url" --argjson started_at_ms "$now" \
    --argjson http_status "$status" --arg content_type "$content_type" --argjson bytes "$bytes" \
    --arg sha256 "$digest" '{purpose:$purpose,url:$url,started_at_ms:$started_at_ms,http_status:$http_status,content_type:$content_type,bytes:$bytes,sha256:$sha256}' >> "$LEDGER"
  rm -f "$headers"
}

taxonomy_rows() {
  jq -ce '
    def rows: [.. | objects | select(has("Name") or has("name") or has("Id") or has("id"))];
    rows
    | if length == 0 then error("taxonomy contains no rows") else . end
    | map(
        (if has("Name") and has("name") then (.Name|type)=="string" and (.name|type)=="string" and .Name==.name
         elif has("Name") then (.Name|type)=="string"
         elif has("name") then (.name|type)=="string" else false end) as $valid_name
        | (if has("Id") and has("id") then (.Id|type)=="string" and (.id|type)=="string" and .Id==.id
           elif has("Id") then (.Id|type)=="string"
           elif has("id") then (.id|type)=="string" else false end) as $valid_id
        | if ($valid_name and $valid_id) | not then error("incomplete or inconsistent taxonomy pair")
          else {name:(.Name // .name),id:(.Id // .id)}
          end
        | if (.id | test("^[A-Za-z0-9-]+$") | not) then error("unsafe taxonomy ID") else . end)
  ' "$1"
}

exact_id() {
  local file=$1 label=$2
  taxonomy_rows "$file" | jq -er --arg label "$label" '[.[]|select(.name==$label)|.id]|unique|if length==1 then .[0] else error("taxonomy exact-match failure: "+$label) end'
}

doc_ratio="$RAW/01-ratio.html"
doc_agreement="$RAW/02-agreement.html"
doc_disclaimer="$RAW/03-disclaimer.html"
request ratio_document html 'https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html' "$doc_ratio"
request user_agreement html 'https://kf.dd373.com/helpdetail/78dba8ca31b44712aef8e923f6c61984.html' "$doc_agreement"
request disclaimer html 'https://about.dd373.com/Index.html?catenum=3732&childcatenum=37325' "$doc_disclaimer"
if LC_ALL=C grep -Eiq '禁止.{0,24}(爬虫|抓取|自动化采集|数据采集)|不得.{0,24}(爬虫|抓取|自动化采集)' "$doc_agreement" "$doc_disclaimer"; then
  echo 'official terms contain an explicit bounded automated-read prohibition marker' >&2
  exit 1
fi

game="$RAW/04-game.json"
areas="$RAW/05-areas.json"
servers="$RAW/06-servers.json"
roots="$RAW/07-roots.json"
runes="$RAW/08-runes.json"
uniques="$RAW/09-uniques.json"
sets="$RAW/10-sets.json"
request game_list json 'https://game.dd373.com/api/game/list' "$game"
game_id=$(exact_id "$game" "$GAME_LABEL")
jq -e --arg label "$GAME_LABEL" --arg id "$game_id" '[..|objects|select((.Name//.name)==$label and (.Id//.id)==$id)]
  | length==1 and ((.[0].IsClose//.[0].isClose//false)==false) and ((.[0].CanTrade//.[0].canTrade//true)==true) and ((.[0].IsEnabled//.[0].isEnabled//true)==true)' "$game" >/dev/null \
  || { echo 'exact game is not uniquely enabled and tradeable' >&2; exit 1; }
request areas json "https://game.dd373.com/Api/GameOther/List?parentId=$game_id" "$areas"
area_id=$(exact_id "$areas" "$AREA_LABEL")
request servers json "https://game.dd373.com/Api/GameOther/List?parentId=$area_id" "$servers"
server_id=$(exact_id "$servers" "$SERVER_LABEL")
if taxonomy_rows "$servers" | jq -e 'any(.[];.name|IN("非赛季普通","非赛季专家","新赛季普通","赛季普通","新赛季专家","赛季专家")) and any(.[];.name=="非赛季(术士君临)")' >/dev/null; then :; fi
request goods_roots json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$game_id" "$roots"
rune_root=$(exact_id "$roots" '符文')
unique_root=$(exact_id "$roots" '暗金装备&饰品')
set_root=$(exact_id "$roots" '套装')
request rune_children json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$rune_root" "$runes"
request unique_children json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$unique_root" "$uniques"
request set_children json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$set_root" "$sets"
rune_leaf=$(exact_id "$runes" '17号符文')
unique_leaf=$(exact_id "$uniques" '武器')
set_leaf=$(exact_id "$sets" '法师')
realm_path="${area_id}_${server_id}"

capture_listing() {
  local sequence=$1 family=$2 canonical=$3 leaf=$4 raw result url
  raw="$RAW/$sequence-listing.json"
  result="$CACHE_ROOT/$sequence-family.json"
  url="https://goods.dd373.com/Api/Goods/UserCenter/ApiGetShopList?gameid=$game_id&GameOtherId=$realm_path&GameShopTypeId=$leaf"
  request "${family}_listing" json "$url" "$raw"
  summarize_one "$family" "$canonical" "$CATALOG" "$raw" "$result"
  jq -c . "$result" >> "$SEGMENTS"
  rm -f "$raw" "$result"
}
capture_listing 11 rune 'base:r17' "$rune_leaf"
capture_listing 12 unique 'unique:The Oculus' "$unique_leaf"
capture_listing 13 set-item "set-item:Tal Rasha's Adjudication" "$set_leaf"
(( request_count == 13 )) || { echo "expected exactly 13 requests, observed $request_count" >&2; exit 1; }
[[ $(wc -l < "$LEDGER") -eq 13 ]] || { echo 'request ledger count mismatch' >&2; exit 1; }
[[ ! -e "$RAW/11-listing.json" && ! -e "$RAW/12-listing.json" && ! -e "$RAW/13-listing.json" ]] || { echo 'raw listing body retained' >&2; exit 1; }

captured_at=$(date -u +%FT%TZ)
build_report live_capture "$captured_at" "$SEGMENTS" "$CACHE_ROOT/report.json.tmp"
jq -S -s --arg captured_at "$captured_at" --arg user_agent "$USER_AGENT" \
  --arg game "$GAME_LABEL" --arg area "$AREA_LABEL" --arg server "$SERVER_LABEL" '
  {
    manifest_version:1,captured_at:$captured_at,user_agent:$user_agent,
    request_count:length,minimum_start_interval_ms:1100,
    scope:{game:$game,season:$area,play_mode:"normal",server:$server},
    requests:.,raw_responses_retained:false,privacy_exclusion_count:0
  }
  ' "$LEDGER" > "$CACHE_ROOT/manifest.unfixed.json"
privacy_count=$(jq '[.families[].counts.privacy_excluded]|add' "$CACHE_ROOT/report.json.tmp")
jq -S --argjson privacy "$privacy_count" '.privacy_exclusion_count=$privacy' "$CACHE_ROOT/manifest.unfixed.json" > "$CACHE_ROOT/manifest.json.tmp"
validate_retained "$CACHE_ROOT/manifest.json.tmp"
validate_manifest_pacing "$CACHE_ROOT/manifest.json.tmp"
mv "$CACHE_ROOT/report.json.tmp" "$REPORT"
mv "$CACHE_ROOT/manifest.json.tmp" "$MANIFEST"
rm -f "$CACHE_ROOT/manifest.unfixed.json"
validate_retained "$REPORT"
validate_retained "$MANIFEST"
cleanup_coordinate
trap - EXIT
[[ ! -e "$CACHE_ROOT" ]] || { echo 'temporary coordinate cleanup failed' >&2; exit 1; }
