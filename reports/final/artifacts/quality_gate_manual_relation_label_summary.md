# Manual Relation Labeling Summary

Generated at unix ms: `1778618348390`

Total samples: `134`

Labeled samples: `0`

Unlabeled samples: `134`

Unsupported samples: `0`

Recall estimate: `unknown_no_gold_false_negative_denominator`

## Workflow

Edit sampled Markdown bullets with values like `yes`, `true`, or `x`, then run `codegraph-mcp audit label-samples` followed by `codegraph-mcp audit summarize-labels`. Unsupported patterns may use `- unsupported: yes` and `- unsupported_pattern: <pattern>`.

## Inputs

### Edge JSON

- `reports/final/artifacts/run_20260510_232527/sample_CALLS.json`
- `reports/final/artifacts/run_20260510_232527/sample_READS.json`
- `reports/final/artifacts/run_20260510_232527/sample_WRITES.json`
- `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json`

### PathEvidence JSON

- `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json`

### Label Markdown

- `reports/final/artifacts/run_20260510_232527/sample_CALLS.md`
- `reports/final/artifacts/run_20260510_232527/sample_READS.md`
- `reports/final/artifacts/run_20260510_232527/sample_WRITES.md`
- `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.md`
- `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.md`

## Precision By Relation

| Relation | Labeled | Unlabeled | Unsupported | Unsure | TP | FP | Wrong span | Precision | Recall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `CALLS` | 0 | 30 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |
| `FLOWS_TO` | 0 | 50 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |
| `PathEvidence` | 0 | 20 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |
| `READS` | 0 | 31 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |
| `WRITES` | 0 | 3 | 0 | 0 | 0 | 0 | 0 | unknown | `unknown_no_gold_false_negative_denominator` |

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
| `edge` | `CALLS` | 1 | `edge-key:-9203119335740638116` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 2 | `edge-key:-8696642248883918410` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 3 | `edge-key:-8662264075891754807` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 4 | `edge-key:-8179267985489797771` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 5 | `edge-key:-7122978112970569375` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 6 | `edge-key:-6090242022326402403` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 7 | `edge-key:-5697957415253836435` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 8 | `edge-key:-4831390274733362438` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 9 | `edge-key:-4173915550597516060` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 10 | `edge-key:-3629816440212955401` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 11 | `edge-key:-2540463547328484458` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 12 | `edge-key:-2524722484245579099` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 13 | `edge-key:-2075607768432037724` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 14 | `edge-key:-2074202350736276718` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 15 | `edge-key:-1562217487425745418` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 16 | `edge-key:-1342439232913117547` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 17 | `edge-key:-1202743239963183456` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 18 | `edge-key:-668374398689844391` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 19 | `edge-key:-401414492922706285` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 20 | `edge://12200ba7e3282f58751724ead67a4a92` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 21 | `edge://3f647be31db88b514e20f120eff2133f` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 22 | `edge://5fe8cf8b0e910d8de4787d002e2324ef` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 23 | `edge://75221d107a591d340593138ea77be632` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 24 | `edge://9b68e7cc497dadbc640e6a51d1ec88da` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 25 | `edge://a1535acbeacf4155df648463bdbd2e4f` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 26 | `edge://aa9bc1c26433df231810f2923a3d4545` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 27 | `edge://c592b76791d136e522e70edff7056ef3` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 28 | `edge://cfab76bb0580290e6a2f79ff675b2b50` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 29 | `edge://d691f242ee0f318408f475bc7997882e` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `CALLS` | 30 | `edge://fbd5ddd96d214c416810511f85c63f33` | `reports/final/artifacts/run_20260510_232527/sample_CALLS.json` |
| `edge` | `FLOWS_TO` | 1 | `edge-key:-8968814412825094128` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 2 | `edge-key:-8932595346684089157` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 3 | `edge-key:-8931892587166734238` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 4 | `edge-key:-8882367084918076338` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 5 | `edge-key:-8862595974082022165` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 6 | `edge-key:-8701454121290662982` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 7 | `edge-key:-8523535827599030407` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 8 | `edge-key:-8514931106740183051` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 9 | `edge-key:-8505375487769567490` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 10 | `edge-key:-8280935406507169843` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 11 | `edge-key:-8193059087951499845` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 12 | `edge-key:-7616031965667234723` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 13 | `edge-key:-7591096624008635989` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 14 | `edge-key:-7412905055993568993` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 15 | `edge-key:-7069541534056199184` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 16 | `edge-key:-6997957043352048496` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 17 | `edge-key:-6934739664744838281` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 18 | `edge-key:-6810267495257665014` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 19 | `edge-key:-6706365188965007484` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 20 | `edge-key:-6494554155358846890` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 21 | `edge-key:-6441590203583740025` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 22 | `edge-key:-6437636834494543462` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 23 | `edge-key:-6396346272525705294` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 24 | `edge-key:-6160310627128939728` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 25 | `edge-key:-6087773232331521904` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 26 | `edge-key:-5954688276726876742` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 27 | `edge-key:-5604131598595792980` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 28 | `edge-key:-5551799344325528117` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 29 | `edge-key:-5348629650215903429` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 30 | `edge-key:-5320812356318544599` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 31 | `edge-key:-5140489319949848871` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 32 | `edge-key:-5126588501475850851` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 33 | `edge-key:-5083306148284553816` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 34 | `edge-key:-5024586843951785344` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 35 | `edge-key:-4985521945923698675` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 36 | `edge-key:-4922606014487661581` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 37 | `edge-key:-4921715565400873968` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 38 | `edge-key:-4629578602212942928` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 39 | `edge-key:-4576367243493334573` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 40 | `edge-key:-4458932643606904775` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 41 | `edge-key:-4250109911766913739` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 42 | `edge-key:-4155883514244134927` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 43 | `edge-key:-4026250569370077681` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 44 | `edge-key:-3974071443315218837` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 45 | `edge-key:-3703913481392354043` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 46 | `edge-key:-3325242497072510788` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 47 | `edge-key:-3300763334872427753` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 48 | `edge-key:-2914225770881291994` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 49 | `edge-key:-2830140751435055111` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `FLOWS_TO` | 50 | `edge-key:-2735513688193329234` | `reports/final/artifacts/run_20260510_232527/sample_FLOWS_TO.json` |
| `edge` | `READS` | 1 | `edge-key:-8951574106955898787` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 2 | `edge-key:-8830330131378442026` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 3 | `edge-key:-7498651423641075307` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 4 | `edge-key:-7085950704099197853` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 5 | `edge-key:-6773604984353227930` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 6 | `edge-key:-6387565098441156913` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 7 | `edge-key:-6055961913509234733` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 8 | `edge-key:-5430917370423224198` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 9 | `edge-key:-4909498656739122022` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 10 | `edge-key:-4788022195110301196` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 11 | `edge-key:-4616353073752889738` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 12 | `edge-key:-3945652436233745553` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 13 | `edge-key:-3880587309966665884` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 14 | `edge-key:-3725722781358078594` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 15 | `edge-key:-3225699846625946388` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 16 | `edge-key:-2836162608334653390` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 17 | `edge-key:-2825872457515553417` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 18 | `edge-key:-2822560138677879045` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 19 | `edge-key:-2772153859073977003` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 20 | `edge-key:-2564217349532046620` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 21 | `edge-key:-2169610515863014134` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 22 | `edge-key:-2003782212418409357` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 23 | `edge-key:-2001264255659101451` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 24 | `edge-key:-1918073339163376418` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 25 | `edge-key:-1604816454417718436` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 26 | `edge-key:-1393554533152118966` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 27 | `edge-key:-1078538732771348825` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 28 | `edge-key:-1055227141490625092` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 29 | `edge-key:-744270777804890043` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 30 | `edge-key:-458875916231284018` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `READS` | 31 | `edge-key:-331606334840065404` | `reports/final/artifacts/run_20260510_232527/sample_READS.json` |
| `edge` | `WRITES` | 1 | `edge-key:-7412890628781895775` | `reports/final/artifacts/run_20260510_232527/sample_WRITES.json` |
| `edge` | `WRITES` | 2 | `edge-key:-5748113415438238707` | `reports/final/artifacts/run_20260510_232527/sample_WRITES.json` |
| `edge` | `WRITES` | 3 | `edge-key:-997635485439463650` | `reports/final/artifacts/run_20260510_232527/sample_WRITES.json` |
| `path` | `PathEvidence` | 1 | `generated://audit/edge-key:-9203119335740638116` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 2 | `generated://audit/edge-key:-9186605581599358930` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 3 | `generated://audit/edge-key:-9184959491801231768` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 4 | `generated://audit/edge-key:-9183397129905989238` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 5 | `generated://audit/edge-key:-9167314706337965205` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 6 | `generated://audit/edge-key:-9167193223427871390` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 7 | `generated://audit/edge-key:-9158827528441336654` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 8 | `generated://audit/edge-key:-9147749043063313696` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 9 | `generated://audit/edge-key:-9134365703709946445` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 10 | `generated://audit/edge-key:-9120530291440236237` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 11 | `generated://audit/edge-key:-9118877849179843568` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 12 | `generated://audit/edge-key:-9109762047521311757` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 13 | `generated://audit/edge-key:-9103623495338762252` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 14 | `generated://audit/edge-key:-9088743095391623643` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 15 | `generated://audit/edge-key:-9078797523463538200` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 16 | `generated://audit/edge-key:-9063415924526603170` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 17 | `generated://audit/edge-key:-9022168102169518164` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 18 | `generated://audit/edge-key:-9003926772312636661` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 19 | `generated://audit/edge-key:-8968814412825094128` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |
| `path` | `PathEvidence` | 20 | `generated://audit/edge-key:-8965697013326455734` | `reports/final/artifacts/run_20260510_232527/sample_PathEvidence.json` |

## Notes

- Aggregated 1 labeled-sample file(s).
- Blank labels remain unlabeled and are excluded from precision denominators.
- Manual labels are read from edited sample markdown bullets or from sample JSON manual_labels/labels objects.
- No human labels were found in the supplied inputs.
- Recall is unknown unless a separate gold false-negative denominator is supplied; sampled positives only estimate precision.
- No labeled samples found; precision and recall remain unknown.
