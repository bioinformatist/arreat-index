#!/usr/bin/env bash
set -euo pipefail

[[ $# == 3 ]] || { echo 'usage: build-name-catalog.sh SNAPSHOT ALIASES OUTPUT' >&2; exit 2; }
snapshot=$1
aliases=$2
output=$3
[[ -f "$snapshot" && -f "$aliases" && ! -d "$output" ]] || { echo 'snapshot and aliases must be files and output must not be a directory' >&2; exit 2; }
command -v jq >/dev/null && command -v opencc >/dev/null || { echo 'jq and OpenCC are required' >&2; exit 2; }
opencc_version=$(opencc --version 2>&1 | sed -n 's/^Version: //p')
[[ "$opencc_version" == '1.3.0' ]] || { echo "OpenCC 1.3.0 required, found ${opencc_version:-unknown}" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for pair in '塔拉夏的判決|塔拉夏的判决' '吉永之臉|吉永之脸' '蛇魔法師之皮|蛇魔法师之皮' '馬拉的萬花筒|马拉的万花筒'; do
  input=${pair%%|*}; expected=${pair#*|}
  actual=$(printf '%s' "$input" | opencc -c tw2s.json)
  [[ "$actual" == "$expected" ]] || { echo "OpenCC tw2s sentinel failed" >&2; exit 1; }
done

jq -e '
  .schema_version == 1 and (.canonical_items | type == "array")
  and (.build | type == "object")
  and all(.canonical_items[]; (.id|type)=="string" and (.names|type)=="array")
' "$snapshot" >/dev/null || { echo 'invalid Schema v1 snapshot' >&2; exit 1; }

jq -e --slurpfile snapshot "$snapshot" '
  def norm: gsub("０";"0")|gsub("１";"1")|gsub("２";"2")|gsub("３";"3")|gsub("４";"4")|gsub("５";"5")|gsub("６";"6")|gsub("７";"7")|gsub("８";"8")|gsub("９";"9")|ascii_downcase|gsub("[\\p{P}\\s]";"");
  (.version == 1) and (.entries|type == "array") and (.entries|length > 0)
  and all(.entries[]; . as $entry
    | (.canonical_id|type)=="string" and (.alias|type)=="string" and ((.alias|norm)!="")
    and (.kind|IN("abbreviation","legacy_simplified","common_misspelling","market_shorthand"))
    and .provenance=="bounded_dd373_observation_2026-08-22"
    and ([$snapshot[0].canonical_items[].id] | index($entry.canonical_id)) != null)
  and ([.entries[]|{key:(.alias|norm),id:.canonical_id}] | group_by(.key) | all(.[]; ([.[].id]|unique|length)==1))
' "$aliases" >/dev/null || { echo 'invalid, conflicting, or missing-target alias map' >&2; exit 1; }

jq -c '[.canonical_items[] as $item | $item.names[]
  | select(.locale|IN("enUS","zhTW","zhCN"))
  | select((.text|type)=="string" and .text!="")
  | {id:$item.id,name:.text,source:"official"}] | unique_by([.id,.name,.source])' "$snapshot" > "$tmp/official.json"
jq -r '.canonical_items[] as $item | $item.names[] | select(.locale=="zhTW") | .text' "$snapshot" > "$tmp/tw.txt"
opencc -c tw2s.json -i "$tmp/tw.txt" -o "$tmp/s.txt"
jq -c '[.canonical_items[] as $item | $item.names[] | select(.locale=="zhTW") | $item.id]' "$snapshot" > "$tmp/tw-ids.json"
jq -Rsc 'split("\n")[:-1]' "$tmp/s.txt" > "$tmp/s-texts.json"
jq -cn --slurpfile ids "$tmp/tw-ids.json" --slurpfile texts "$tmp/s-texts.json" '
  if ($ids[0]|length) != ($texts[0]|length) then error("OpenCC output cardinality changed")
  else [range(0;$ids[0]|length) as $i | {id:$ids[0][$i],name:$texts[0][$i],source:"opencc"}]
  end | unique_by([.id,.name,.source])' > "$tmp/opencc.json"
jq -c '[.entries[]|{id:.canonical_id,name:.alias,source:"community"}]|unique_by([.id,.name,.source])' "$aliases" > "$tmp/community.json"

jq -S --slurpfile official "$tmp/official.json" --slurpfile opencc "$tmp/opencc.json" \
  --slurpfile community "$tmp/community.json" --slurpfile aliases "$aliases" \
  --arg opencc_version "$opencc_version" '
  def norm:
    gsub("０";"0") | gsub("１";"1") | gsub("２";"2")
    | gsub("３";"3") | gsub("４";"4") | gsub("５";"5")
    | gsub("６";"6") | gsub("７";"7") | gsub("８";"8")
    | gsub("９";"9") | ascii_downcase | gsub("[\\p{P}\\s]";"");
  ($official[0] + $opencc[0] + $community[0]
    | map({id, source, normalized_name:(.name | norm)})
    | map(select(.normalized_name != ""))
    | unique_by([.id, .normalized_name, .source])) as $candidates
  | ($candidates | map(select(.id | startswith("unique:")))) as $unique
  | ($candidates | map(select(.id | startswith("set-item:")))) as $set
  |
  {
    catalog_version:1,
    snapshot:{schema_version:.schema_version,build:.build,canonical_item_count:(.canonical_items|length)},
    canonical_ids:[.canonical_items[].id],
    opencc:{version:$opencc_version,config:"tw2s.json"},
    alias_map:{version:$aliases[0].version,count:($aliases[0].entries|length)},
    candidate_count:($candidates | length),
    candidate_groups:{unique:$unique,set:$set,mixed:($unique + $set)}
  }' "$snapshot" > "$tmp/catalog.json"
mv "$tmp/catalog.json" "$output"
