# Autoresearch Update Repro Fix

Verdict: `passed`

Update mode: `update-fast`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `small_fixture` | `passed` | 20 | `true` | `true` | `true` | `true` |
| `medium_fixture` | `passed` | 20 | `true` | `true` | `true` | `true` |

## Step Metrics

### `small_fixture`

Mutation file: `src/service.ts`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 527 | 2 | 2 | 2 | 2 | 19 | 40 | 3 | `ok` |
| `repeat_unchanged_index` | 14 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 36 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 32 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 34 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 30 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 29 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 31 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 34 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 30 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 35 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 35 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 35 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 35 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 29 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 35 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 34 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 40 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 43 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 36 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 33 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 35 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 40 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 31 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 30 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 35 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 27 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 31 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 31 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 36 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 33 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |

### `medium_fixture`

Mutation file: `src/module_000.ts`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 1295 | 48 | 48 | 48 | 48 | 576 | 1296 | 144 | `ok` |
| `repeat_unchanged_index` | 24 | 48 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 310 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 267 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 271 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 286 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 297 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 260 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 278 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 289 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 292 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 293 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 285 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 269 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 274 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 308 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 282 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 275 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 275 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 283 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 330 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 283 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 280 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 263 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 277 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 266 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 289 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 278 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 277 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 253 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 279 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 258 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 266 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 260 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 272 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 252 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 268 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 258 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 262 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 256 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |
| `single_file_update` | 285 | 1 | 1 | 1 | 1 | 16 | 41 | 138 | `ok` |
| `restore_update` | 286 | 1 | 1 | 1 | 1 | 12 | 30 | 138 | `ok` |

