#!/usr/bin/env bash
set -euo pipefail

objects_with_ids() {
  jq -c '[.. | objects
    | (.Name // .name) as $name
    | (.Id // .id) as $id
    | select(($name|type)=="string" and ($id|type)=="string")
    | {name:$name,id:$id}] | unique_by([.name,.id])' "$1"
}

exact_id() {
  local file=$1 label=$2
  objects_with_ids "$file" | jq -er --arg label "$label" '[.[] | select(.name==$label) | .id] | unique | if length==1 then .[0] else error("taxonomy-name mismatch: "+$label) end'
}

source_entry() {
  local purpose=$1 url=$2 started_at_ms=$3 http_status=$4 content_type=$5 bytes=$6 sha256=$7
  jq -cn --arg purpose "$purpose" --arg url "$url" --argjson started_at_ms "$started_at_ms" \
    --argjson http_status "$http_status" --arg content_type "$content_type" --argjson bytes "$bytes" \
    --arg sha256 "$sha256" \
    '{purpose:$purpose,url:$url,started_at_ms:$started_at_ms,http_status:$http_status,
      content_type:$content_type,bytes:$bytes,sha256:$sha256}'
}

if [[ "${1:-}" == '--fixture-mode' ]]; then
  [[ $# == 1 ]] || { echo 'fixture mode accepts no arguments' >&2; exit 1; }
  fixture='research/dd373/fixtures/matching-api-response.json'
  taxonomy_id=$(exact_id "$fixture" '暗黑2：重制版国服')
  provenance=$(source_entry fixture 'https://example.invalid/synthetic.json' 1700000000123 200 \
    'application/json; charset=utf-8' 321 \
    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef')
  jq -cn --arg taxonomy_id "$taxonomy_id" --argjson source_entry "$provenance" \
    '{taxonomy_id:$taxonomy_id,source_entry:$source_entry}'
  exit 0
fi

readonly UA='Arreat-Index-Matching-Research/0.1'
readonly INTERVAL_MS="${DD373_REQUEST_INTERVAL_MS:-1100}"
readonly GAME_LABEL='暗黑2：重制版国服'
readonly CACHE_ROOT=".cache/arreat-index/dd373-${IMPROVE_EXECUTION_ID:-}"
readonly SANITIZER='research/dd373/sanitize-listings.jq'

[[ -n "${IMPROVE_EXECUTION_ID:-}" ]] || { echo 'IMPROVE_EXECUTION_ID must be nonempty' >&2; exit 1; }
[[ "$INTERVAL_MS" =~ ^[0-9]+$ ]] && (( INTERVAL_MS >= 1000 )) || { echo 'request interval must be at least 1000 ms' >&2; exit 1; }
[[ ! -e "$CACHE_ROOT" ]] || { echo "capture coordinate already exists: $CACHE_ROOT" >&2; exit 1; }
mkdir -p "$CACHE_ROOT/raw"

request_count=0
last_start_ms=0
sources_ndjson="$CACHE_ROOT/sources.ndjson"
samples_ndjson="$CACHE_ROOT/samples.ndjson"
: > "$sources_ndjson"
: > "$samples_ndjson"

cleanup_raw() { rm -f "$CACHE_ROOT/raw"/*; }
trap cleanup_raw EXIT

request() {
  local purpose=$1 kind=$2 url=$3 out=$4 now wait_ms status content_type bytes digest
  [[ "$url" == https://* ]] || { echo "non-HTTPS URL rejected" >&2; exit 1; }
  (( ++request_count <= 32 )) || { echo 'request 33 rejected' >&2; exit 1; }
  now=$(date +%s%3N)
  if (( last_start_ms > 0 && now - last_start_ms < INTERVAL_MS )); then
    wait_ms=$((INTERVAL_MS - (now - last_start_ms)))
    sleep "$(awk -v ms="$wait_ms" 'BEGIN { printf "%.3f", ms / 1000 }')"
    now=$(date +%s%3N)
  fi
  last_start_ms=$now
  status=$(curl --silent --show-error --location --max-redirs 3 --connect-timeout 20 --max-time 60 \
    --user-agent "$UA" --output "$out" --dump-header "$out.headers" --write-out '%{http_code}' "$url") || {
      echo "network request failed without retry: $url" >&2; exit 1;
    }
  [[ "$status" == 200 ]] || { echo "HTTP $status without retry: $url" >&2; exit 1; }
  content_type=$(awk 'BEGIN{IGNORECASE=1} /^content-type:/{sub(/^[^:]*:[[:space:]]*/,""); sub(/\r$/,""); value=$0} END{print value}' "$out.headers")
  if [[ "$kind" == json ]]; then
    [[ "$content_type" == application/json* ]] || { echo "unexpected JSON content type: $content_type" >&2; exit 1; }
    jq -e . "$out" >/dev/null || { echo "invalid JSON response: $url" >&2; exit 1; }
  else
    [[ "$content_type" == text/html* ]] || { echo "unexpected HTML content type: $content_type" >&2; exit 1; }
  fi
  if LC_ALL=C grep -Eiq 'captcha|访问验证|安全验证|登录后|请登录|browser challenge|cloudflare ray' "$out"; then
    echo "login/cookie/challenge marker detected: $url" >&2; exit 1
  fi
  bytes=$(wc -c < "$out" | tr -d ' ')
  (( bytes > 0 )) || { echo "empty response: $url" >&2; exit 1; }
  digest=$(sha256sum "$out" | cut -d' ' -f1)
  source_entry "$purpose" "$url" "$now" "$status" "$content_type" "$bytes" "$digest" >> "$sources_ndjson"
  rm -f "$out.headers"
}

doc_listing="$CACHE_ROOT/raw/doc-listing.html"
doc_agreement="$CACHE_ROOT/raw/doc-agreement.html"
doc_disclaimer="$CACHE_ROOT/raw/doc-disclaimer.html"
request docs html 'https://kf.dd373.com/helpdetail/dc862c7b70d74880968d2386e544b5bf.html' "$doc_listing"
request terms html 'https://kf.dd373.com/helpdetail/78dba8ca31b44712aef8e923f6c61984.html' "$doc_agreement"
request terms html 'https://about.dd373.com/Index.html?catenum=3732&childcatenum=37325' "$doc_disclaimer"
if LC_ALL=C grep -Eiq '禁止.{0,24}(爬虫|抓取|自动化采集|数据采集)|不得.{0,24}(爬虫|抓取|自动化采集)' "$doc_agreement" "$doc_disclaimer"; then
  echo 'official terms contain an explicit bounded-research prohibition marker' >&2; exit 1
fi

game_json="$CACHE_ROOT/raw/game.json"
request game json 'https://game.dd373.com/api/game/list' "$game_json"
game_id=$(exact_id "$game_json" "$GAME_LABEL")
jq -e --arg label "$GAME_LABEL" '[..|objects|select(any(.[]; .==$label))][0]
  | ((.IsClose // .isClose // false)==false)
    and ((.CanTrade // .canTrade // true)==true)
    and ((.IsEnabled // .isEnabled // true)==true)' "$game_json" >/dev/null \
  || { echo 'exact game is not enabled and tradeable' >&2; exit 1; }

goods_root="$CACHE_ROOT/raw/goods-root.json"
realm_servers="$CACHE_ROOT/raw/realm-servers.json"
request taxonomy json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$game_id" "$goods_root"
# The parent is the previously evidenced exact 国服 node; the returned server
# label is still selected afresh and must be unambiguous.
request taxonomy json 'https://game.dd373.com/Api/GameOther/List?parentId=7b1751f92c844871ab80cae0822feea2' "$realm_servers"
realm_id='7b1751f92c844871ab80cae0822feea2'
server_entry=$(objects_with_ids "$realm_servers" | jq -ec '[.[] | select(.name|test("非赛季") and test("普通") and (test("专家")|not))] | unique_by(.id) | if length==1 then .[0] else error("ordinary non-season mainland server ambiguity") end')
server_id=$(jq -r .id <<<"$server_entry")
server_label=$(jq -r .name <<<"$server_entry")
realm_server_path="${realm_id}_${server_id}"

rune_root_id=$(exact_id "$goods_root" '符文')
unique_root_id=$(exact_id "$goods_root" '暗金装备&饰品')
set_root_id=$(exact_id "$goods_root" '套装')
charm_root_id=$(exact_id "$goods_root" '咒符（护身符）')
jewel_root_id=$(exact_id "$goods_root" '珠宝')

runes_json="$CACHE_ROOT/raw/runes.json"
unique_json="$CACHE_ROOT/raw/unique.json"
set_json="$CACHE_ROOT/raw/set.json"
request taxonomy json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$rune_root_id" "$runes_json"
request taxonomy json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$unique_root_id" "$unique_json"
request taxonomy json "https://game.dd373.com/Api/GameGoodsType/List?parentId=$set_root_id" "$set_json"

rune_taxonomy=$(objects_with_ids "$runes_json" | jq -ec '[.[] | select(.name|test("^[0-9]+号符文$"))
  | {number:(.name|capture("^(?<n>[0-9]+)号符文$").n|tonumber),category:.name,id}]
  | unique_by(.number) | sort_by(.number)
  | if length==33 and ([.[].number]==[range(1;34)]) then . else error("rune taxonomy must be exact 1-33") end')
unique_children=$(objects_with_ids "$unique_json" | jq -ec 'unique_by(.id) | if length==9 then . else error("unique taxonomy must have nine children") end')
set_children=$(objects_with_ids "$set_json" | jq -ec 'unique_by(.id) | if length==9 and any(.[];.name=="术士") then . else error("set taxonomy must have nine children including 术士") end')

listing_url() {
  local goods_id=$1
  printf 'https://goods.dd373.com/Api/Goods/UserCenter/ApiGetShopList?gameid=%s&GameOtherId=%s&GameShopTypeId=%s' "$game_id" "$realm_server_path" "$goods_id"
}

capture_listing() {
  local family=$1 category=$2 goods_id=$3 prefix=$4 rune_number=${5:-}
  local raw="$CACHE_ROOT/raw/listing-${prefix}.json"
  request listing json "$(listing_url "$goods_id")" "$raw"
  jq -e '.StatusData.ResultData | type=="array"' "$raw" >/dev/null || { echo 'unexpected listing array shape' >&2; exit 1; }
  jq -cn --arg prefix "$prefix" --arg family "$family" --arg category "$category" \
    --arg rune_number "$rune_number" --slurpfile response "$raw" \
    '{sample_id_prefix:$prefix,family:$family,category:$category,response:$response[0]}
     + (if $rune_number=="" then {} else {rune_number:($rune_number|tonumber)} end)' >> "$samples_ndjson"
  rm -f "$raw"
}

while IFS= read -r child; do
  capture_listing unique "$(jq -r .name <<<"$child")" "$(jq -r .id <<<"$child")" "unique-$(jq -r .id <<<"$child" | cut -c1-8)"
done < <(jq -c '.[]' <<<"$unique_children")
while IFS= read -r child; do
  capture_listing set "$(jq -r .name <<<"$child")" "$(jq -r .id <<<"$child")" "set-$(jq -r .id <<<"$child" | cut -c1-8)"
done < <(jq -c '.[]' <<<"$set_children")
capture_listing mixed '咒符（护身符）' "$charm_root_id" mixed-charm
capture_listing mixed '珠宝' "$jewel_root_id" mixed-jewel
for number in 1 17 33; do
  entry=$(jq -ec --argjson number "$number" '.[]|select(.number==$number)' <<<"$rune_taxonomy")
  capture_listing rune "$(jq -r .category <<<"$entry")" "$(jq -r .id <<<"$entry")" "rune-$number" "$number"
done

(( request_count == 32 )) || { echo "expected exactly 32 requests, observed $request_count" >&2; exit 1; }
jq -s --argjson rune_taxonomy "$(jq '[.[]|{number,category}]' <<<"$rune_taxonomy")" \
  '{samples:.,rune_taxonomy:$rune_taxonomy}' "$samples_ndjson" > "$CACHE_ROOT/sanitizer-input.json"
jq -f "$SANITIZER" "$CACHE_ROOT/sanitizer-input.json" > "$CACHE_ROOT/sanitized-corpus.json.tmp"
mv "$CACHE_ROOT/sanitized-corpus.json.tmp" "$CACHE_ROOT/sanitized-corpus.json"

jq -s --arg captured_at "$(date -u +%FT%TZ)" --arg user_agent "$UA" \
  --argjson request_interval_ms "$INTERVAL_MS" --arg game_label "$GAME_LABEL" \
  --arg server_label "$server_label" --argjson privacy_excluded "$(jq '.privacy_excluded' "$CACHE_ROOT/sanitized-corpus.json")" \
  '{captured_at:$captured_at,user_agent:$user_agent,request_count:length,
    request_interval_ms:$request_interval_ms,game_label:$game_label,realm_label:"国服",
    server_label:$server_label,terms_review:"CONDITIONAL: no explicit prohibition found by narrow inspection; redistribution permission unresolved",
    sources:.,privacy_excluded:$privacy_excluded,raw_responses_retained:false}' \
  "$sources_ndjson" > "$CACHE_ROOT/private-manifest.json"

cleanup_raw
rm -f "$sources_ndjson" "$samples_ndjson" "$CACHE_ROOT/sanitizer-input.json"
rmdir "$CACHE_ROOT/raw"
printf '%s\n' "$CACHE_ROOT"
