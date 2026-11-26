![PASTI](assets/banner.png)

Experimental linter for **PASTI** (Panduan Standardisasi Teks dan Istilah), a draft specification for controlled technical Indonesian (*Bahasa Indonesia Teknis Terkendali*). It implements spec `0.1.0-draft.1`.

> *Linter eksperimental untuk PASTI. Alat ini memeriksa aturan yang dapat diperiksa secara mekanis, melaporkan aturan yang tidak diperiksanya, dan tidak pernah mengubah berkas.*

**Status.** PASTI is an independent draft. It is not affiliated with Badan Bahasa (whose terminology app is also named *Pasti*, Padanan Istilah), and it is not affiliated with or endorsed by ASD/STEMG. No precision, recall, or reader benefit has been measured. The thresholds and word lists are draft policy, not validated results.

## What it does

- **`pasti lint`** checks Markdown (CommonMark plus tables and math) and plain text against the automated PASTI text rules. It reports diagnostics with exact source ranges, plus coverage of what it did and did not evaluate, and can apply an acceptance gate.
- **`pasti compare`** compares a source text with a candidate rewrite and flags possible changes in meaning: modality, aspect, negation, conditions, numbers, units, comparison operators, and conditions lost when a sentence is split (Doc 07).
- **`pasti parse`** parses one statement in the experimental formal profile `PASTI-FOR-EX01` (Doc 10) into a typed AST, using a pinned entity registry. It never executes anything.
- **`pasti count`** shows each sentence and its counted units, so writers can check the length metric.
- **`pasti rules`** lists every rule ID in the spec and whether this tool checks it automatically.

Because a clean run is never proof of conformance (PASTI-CORE-007), the strongest status the tool can report is `diperiksa_otomatis`.

## Install

```sh
cargo install --git https://github.com/rifqi2320/pasti
```

## Usage

```sh
pasti lint docs/manual.md --config pasti.yaml          # human-readable output
pasti lint docs/*.md --format json --all               # full report, including passing results
pasti lint manual.md --gate --require-complete         # CI gate
pasti compare source.txt candidate.txt --gate          # meaning-preservation candidates
pasti parse --text "Matikan pompa_utama." --registry examples/registry-demo.json
pasti count manual.md
```

Text output prints `path:line:column` with a 1-based column. JSON uses the spec coordinates: Unicode code points, `line` from 1, `column_cp` and `offset_cp` from 0, and half-open ranges.

| Exit code | Meaning |
| --- | --- |
| 0 | Check finished; the gate passed or was not enabled. Findings alone never change the code without `--gate`. |
| 1 | Check finished; the gate failed. |
| 2 | An input, configuration, or registry failed. A partial report is marked `partial: true`. |

### Text type (mode)

Rules depend on the text type: `PROC`, `DESC`, `REQ`, `WARN`, or `UI`. Set the default with `default_mode` or `--mode`. Switch it inside Markdown with a directive:

```markdown
<!-- pasti: mode=WARN -->
Permukaan panas dapat menyebabkan luka bakar.

<!-- pasti: mode=PROC -->
1. Matikan pompa utama.

<!-- pasti: exact -->
> Klik Restart.
```

`mode=…` applies until the next mode directive. `exact` protects the next block as an exact-source quote. If the mode is unknown, mode-dependent rules return `not_evaluated` (`MODE_UNKNOWN`); the tool does not pick a threshold silently.

## Rules checked automatically

| Rule | Check | Result |
| --- | --- | --- |
| PASTI-TOK-005 | Sentence length over the review threshold (PROC/WARN > 20, DESC/REQ > 25 units; UI only when configured) | `possible_violation`, review pending; length alone is never a violation |
| PASTI-TOK-002 | Ambiguous abbreviations (`dll.`) are not split, and the uncertainty is reported | `possible_violation`, low |
| PASTI-TOK-001 | Raw HTML blocks are not parsed as prose | `not_evaluated` |
| PASTI-NUM-001 | Decimal point (`2.5`), missing leading zero (`,5`), ambiguous separators (`1.000`) | violation or candidate |
| PASTI-NUM-002 | Number glued to a unit (`80°C`, `5kg`) or a miscased unit (`5 KG`) | violation or candidate |
| PASTI-NUM-003 | Bound phrases that do not say strict or inclusive (`di atas 80`, `minimal 5`) | candidate |
| PASTI-NUM-004 | Ranges without inclusivity or unit (`10–20 °C`); references such as `langkah 4–6` are skipped | candidate |
| PASTI-GRA-001 | Affix and preposition spelling from a fixed list without alternative readings (`disini`, `di pasang`), plus project pairs | violation |
| PASTI-GRA-011 | Reduplication written as `kabel2` or `baut baut` | candidate |
| PASTI-BAS-003 | Glossary term status: deprecated, restricted, proposed (needs a glossary) | violation, candidate, or `not_evaluated` |
| PASTI-LEX-005 | A form that maps to several concepts in the glossary | candidate |
| PASTI-LEX-007 | Conflicts between glossary files, which are never resolved by "last file wins" | violation |
| PASTI-SEM-001..004, NUM-006 | Meaning changes between source and rewrite (`pasti compare`) | candidates only |
| PASTI-FOR-001..006 | Formal profile EX01 (`pasti parse`) | accept, or reject with a reason code |

The remaining mandatory text rules are listed in `coverage.mandatory_rules_requiring_manual_review`. Examples include actors and references (BAS-002, BAS-007), separate actions (BAS-004), imperatives (GRA-003), and modality and negation in single texts (GRA-007, GRA-008). They need a human reviewer, and the report keeps them visible.

### Counting (Doc 06)

Units are whitespace-separated tokens, including NBSP and narrow NBSP, that contain a letter, a digit, or a unit symbol. Under this rule:

- `sensor-sensor`, `katupnya`, `2,5` and `80°C` each count as one unit; `80 °C` counts as two.
- Inline code, inline math, URLs and images each count as one atom.
- Punctuation and operators that stand alone (`>`, `±`, `–`) are not counted.
- Code blocks, exact quotes and display equations are protected. They are listed in the report, but the prose rules are not applied to them.

The sentence boundary ignores decimals, versions, ellipses and registered abbreviations. The abbreviation lists are configurable under `abbreviations`.

## Configuration

```yaml
config_version: 1
spec_version: 0.1.0-draft.1
profile_id: documentation
profile_version: 0.1.0-draft.1
default_mode: DESC
length_review: { PROC: 20, WARN: 20, DESC: 25, REQ: 25, UI: null }
gate:
  fail_on: error                    # error | warning | info
  mandatory_checks_complete: false  # true: fail while reviews are pending or checks are not evaluated
  # scope: [PASTI-TOK-005, ...]     # default: automated text rules (glossary rules only if a glossary is set)
fix: { automatic: false }           # autofix is not implemented; true is rejected
glossaries: [glossary-seed.json]
reviews: reviews.yaml
spelling_pairs: [{ wrong: dipanel, right: di panel }]
defined_comparisons: [minimal]      # phrases the profile has defined
disabled_rules: []                  # reported as not_evaluated, never hidden
```

Settings merge in this order: built-in profile, then the project file (`--config`, or `./pasti.yaml`), then command-line flags. The effective configuration, the hashes of the glossaries and sources, and the tool version are all recorded in the JSON report.

## Glossaries

A glossary file is `{"glossary_id", "glossary_version", "entries": [...]}`, where each entry follows the Doc 05 schema. Loading fails with exit code 2 if an entry violates the schema, for example an approved entry without `review_record_id` or a restricted entry without constraints. `examples/glossary-seed.json` holds the 21 seed concepts from Doc 05, all with status `proposed`. Terms not found in a glossary are never flagged as violations.

## Human decisions (reviews, exceptions, deviations, suppression)

The tool never writes decisions. People record them in a YAML file bound to the rule and to the exact text of the scope, using the hash after `:` in the diagnostic's `scope_id`:

```yaml
records:
  - record_id: R-1
    kind: review          # review | exception | deviation | suppression
    state: accepted       # proposed | accepted | rejected
    rule_id: PASTI-TOK-005
    scope_hash: 3f1c2a9b0d4e5f61
    decision: satisfied   # required for review
    reason: Pemecahan kalimat akan memisahkan syarat dari tindakan.
    reviewed_by: [Nama (penelaah bahasa)]
```

Each kind of record has a different effect:

- **`review`** completes the review with the recorded decision.
- **`exception`** requires a `basis` and marks the scope `satisfied`.
- **`deviation`** keeps the violation, but the gate's `fail_on` check no longer counts it.
- **`suppression`** only hides the result in the display. It is still counted in coverage and in the gate.

If the text changes, the record becomes stale. Stale records are listed in the report and are not applied.

## Formal profile EX01

`examples/registry-demo.json` is the demo registry from Doc 10. Accepted statements produce the Doc 10 AST. Rejected statements return a reason code with no AST: `FOR_SYNTAX`, `FOR_UNKNOWN_REFERENCE`, `FOR_CAPABILITY_MISMATCH`, `FOR_UNIT_MISMATCH`, or `FOR_OWNER_MISMATCH`. The profile does no unit conversion and no guessing, and never executes anything.

## Development

```sh
cargo test     # unit tests and Doc 09 fixtures F01–F27
cargo clippy --all-targets
```

The source is in `src/`:

- `document.rs`: Markdown and text adapters with a source map.
- `segment.rs`: sentence splitting and unit counting.
- `patterns.rs`: the detectors.
- `checks.rs`: per-scope results.
- `report.rs`: coverage and the gate.
- `reviews.rs`: decision records.
- `glossary.rs`, `compare.rs`, `formal.rs`.

## License

No license has been chosen yet. The licensing policy is an open decision in the PASTI governance draft (Doc 11). Until a license is added, all rights are reserved.
