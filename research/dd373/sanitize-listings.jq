def fail($message): $message | error;

def forbidden_title:
  test("[[:alnum:]._%+-]+@[[:alnum:].-]+\\.[[:alpha:]]{2,}"; "i")
  or test("https?://|www\\."; "i")
  or test("[0-9０-９]{5,}")
  or test("qq|微信|vx|v信|telegram|discord|手机号|电话"; "i");

if type != "object" or (.samples | type) != "array" then
  fail("sanitizer input must contain a samples array")
elif any(.samples[];
    (.sample_id_prefix | type) != "string"
    or (.family | IN("unique", "set", "mixed", "rune") | not)
    or (.category | type) != "string"
    or (.response.StatusData.ResultData | type) != "array") then
  fail("sample metadata or StatusData.ResultData shape is invalid")
else
  . as $input
  | [
      $input.samples[] as $sample
      | $sample.response.StatusData.ResultData
      | to_entries[]
      | select((.value.title | type) == "string")
      | {
          sample_id: ($sample.sample_id_prefix + "-" + ((.key + 1) | tostring)),
          family: $sample.family,
          category: $sample.category,
          rune_number: $sample.rune_number,
          title: .value.title
        }
    ] as $all
  | {
      records: [
        $all[]
        | select((.title | forbidden_title) | not)
        | if .rune_number == null then del(.rune_number) else . end
      ],
      privacy_excluded: ([$all[] | select(.title | forbidden_title)] | length),
      rune_taxonomy: ($input.rune_taxonomy // [])
    }
end
