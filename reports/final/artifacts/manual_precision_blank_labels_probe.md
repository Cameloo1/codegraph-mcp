# Manual Relation Labeling Summary

Generated at unix ms: `1778644060202`

Total samples: `70`

Labeled samples: `0`

Unlabeled samples: `70`

Unsupported samples: `0`

Recall estimate: `unknown_no_gold_false_negative_denominator`

## Workflow

Edit sampled Markdown bullets with values like `yes`, `true`, or `x`, then run `codegraph-mcp audit label-samples` followed by `codegraph-mcp audit summarize-labels`. Unsupported patterns may use `- unsupported: yes` and `- unsupported_pattern: <pattern>`.

## Inputs

### Edge JSON

- `reports/final/artifacts/manual_precision_sample_CALLS.json`

### PathEvidence JSON

- `reports/final/artifacts/manual_precision_sample_PathEvidence.json`

### Label Markdown

- `reports/final/artifacts/manual_precision_sample_CALLS.md`
- `reports/final/artifacts/manual_precision_sample_PathEvidence.md`

## Precision By Relation

| Relation | Labeled | Unlabeled | Unsupported | Unsure | TP | FP | Wrong span | Precision | Recall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `CALLS` | 0 | 50 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |
| `PathEvidence` | 0 | 20 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |

## Source-Span Precision

| Eligible | Correct span | Wrong span | Precision | Recall |
| ---: | ---: | ---: | ---: | --- |
| 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |

## False-Positive Taxonomy

| Category | Count |
| --- | ---: |
| none | 0 |

## Wrong-Span Taxonomy

| Category | Count |
| --- | ---: |
| none | 0 |

## Unsupported Pattern Taxonomy

| Category | Count |
| --- | ---: |
| none | 0 |

## Unlabeled Samples

| Type | Relation | Ordinal | Sample ID | Source |
| --- | --- | ---: | --- | --- |
| `edge` | `CALLS` | 1 | `edge-key:-9219686753999769194` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 2 | `edge-key:-9218329364481355278` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 3 | `edge-key:-9207033643377571536` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 4 | `edge-key:-9204612225682906844` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 5 | `edge-key:-9203917153979327882` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 6 | `edge-key:-9203193041256805099` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 7 | `edge-key:-9200221164749853995` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 8 | `edge-key:-9194174111317313219` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 9 | `edge-key:-9191016095031720750` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 10 | `edge-key:-9189397946054752705` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 11 | `edge-key:-9188943476723159600` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 12 | `edge-key:-9180476430850508159` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 13 | `edge-key:-9175448200043001236` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 14 | `edge-key:-9173920181625997413` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 15 | `edge-key:-9173892207857001214` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 16 | `edge-key:-9157570605903685370` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 17 | `edge-key:-9155260334438884468` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 18 | `edge-key:-9154203036610331841` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 19 | `edge-key:-9152681992855170393` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 20 | `edge-key:-9152578509063404769` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 21 | `edge-key:-9141581984135587689` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 22 | `edge-key:-9138460836748129052` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 23 | `edge-key:-9130907523969240159` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 24 | `edge-key:-9126065031123564016` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 25 | `edge-key:-9120489193624736370` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 26 | `edge-key:-9112745193504538070` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 27 | `edge-key:-9106514634442348822` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 28 | `edge-key:-9104049820924482317` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 29 | `edge-key:-9103749106578594669` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 30 | `edge-key:-9100751433761581788` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 31 | `edge-key:-9094616922695647144` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 32 | `edge-key:-9084986007038172354` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 33 | `edge-key:-9079661291070287650` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 34 | `edge-key:-9079232366376572273` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 35 | `edge-key:-9072974522009405318` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 36 | `edge-key:-9064041375540811618` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 37 | `edge-key:-9059168678235437306` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 38 | `edge-key:-9057137899338545144` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 39 | `edge-key:-9053502937740038339` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 40 | `edge-key:-9052519494226975129` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 41 | `edge-key:-9050224252766646807` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 42 | `edge-key:-9048160255253830301` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 43 | `edge-key:-9046347112424857064` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 44 | `edge-key:-9038502750529297448` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 45 | `edge-key:-9037548711169898310` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 46 | `edge-key:-9035728481260840138` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 47 | `edge-key:-9031276788898217171` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 48 | `edge-key:-9028314406606205006` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 49 | `edge-key:-9027935844295319503` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `edge` | `CALLS` | 50 | `edge-key:-9024713504740049076` | `reports/final/artifacts/manual_precision_sample_CALLS.json` |
| `path` | `PathEvidence` | 1 | `path://16fe3d901d2767ea` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 2 | `path://6a6110e78b5b5221` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 3 | `path://0a5cc4db3c8ecdf3` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 4 | `path://14e3df0913748040` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 5 | `path://5e358951773480f7` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 6 | `path://ac209085fc2be091` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 7 | `path://46ce5e87a3d7b509` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 8 | `path://73e648035c67d03b` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 9 | `path://2a53d300dd4de481` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 10 | `path://e39d2cdfc44468d2` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 11 | `path://37d9d1b1c9ab3940` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 12 | `path://f9966baea768ab8d` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 13 | `path://833b30ec1b5129d0` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 14 | `path://6ad86dc34719852d` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 15 | `path://b7f7ae0fe0f196b8` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 16 | `path://75cfc9e03af93e95` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 17 | `path://4db98dca38c6ef58` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 18 | `path://3289154380c9d55c` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 19 | `path://1d91ba79d190cf3a` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |
| `path` | `PathEvidence` | 20 | `path://40b41d54fbcb4026` | `reports/final/artifacts/manual_precision_sample_PathEvidence.json` |

## Notes

- Manual labels are read from edited sample markdown bullets or from sample JSON manual_labels/labels objects.
- Blank labels remain unlabeled and are excluded from precision denominators.
- Recall is unknown unless a separate gold false-negative denominator is supplied; sampled positives only estimate precision.
- No human labels were found in the supplied inputs.
- No labeled samples found; precision and recall remain unknown.
