# D2R Affix Data

This context describes how affix property applications retain source facts and gain evidence-backed meanings.

## Language

**Affix Modifier**:
One property application attached to a prefix, suffix, or automatic affix.
_Avoid_: Roll for the entire modifier

**Source Operands**:
The verbatim parameter, Min, and Max cells stored for an Affix Modifier in the game tables.
_Avoid_: Range, because the cells can encode different quantities

**Modifier Interpretation**:
A typed, provenance-backed meaning derived from property metadata while Source Operands remain unchanged.
_Avoid_: Parsed range as a universal term

**Numeric Roll Range**:
Inclusive numeric lower and upper bounds only when property metadata establishes that Min and Max are endpoints.
_Avoid_: Applying it to skill identifiers, trigger chance and level, charges and level, or per-level encodings

**Triggered Skill Effect**:
A chance-to-cast interpretation containing a skill, effective trigger percentage, and skill level. “Proc” is a community synonym.
_Avoid_: Proc as the canonical term

**Charged Skill Effect**:
An interpretation containing a skill, maximum charges, and skill level.

**Item-level Scaled Charged Skill Effect**:
A charged skill whose effective skill level and maximum charges derive from the item's level and the skill's required level while its Source Operands remain unchanged.
_Avoid_: Negative charges, fixed charged skill

**Uninterpreted Modifier**:
An Affix Modifier whose Source Operands are valid but whose property metadata is not yet safely mapped to a typed meaning.
_Avoid_: Invalid modifier

**Evidence Sentinel**:
A named fixture or product evidence assertion reported by audit but not itself a universal data-integrity invariant.

**Current Ask**:
An active seller listing amount observed at one time. It is not a transaction, history, valuation, or recommendation.

**Comparable Current Ask**:
A non-private, single-canonical-item listing with a positive exact per-unit amount and one consistent upstream unit.

**Current Ask Summary**:
An aggregate observation containing sample counts, exclusions, minimum and median comparable current asks; it retains no listing title, seller, contact, or raw response.
