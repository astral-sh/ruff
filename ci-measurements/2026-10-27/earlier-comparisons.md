# Earlier Ruff action and cache comparisons

These measurements use the action revisions recorded below, before workspace cache keys included a source fingerprint. Their timings apply to those earlier pins and measured cache states. The refreshing action is measured separately in [the current report](README.md).

Parentheses in the timing tables show 90% intervals for mean time saved; negative savings mean a slowdown. † marks jobs shortened to setup and build steps.

<details>
<summary>Warm-cache timings after source changes</summary>

`Before → after` compares dependency-only caching with `Swatinem/rust-cache@6323deb1` against the shared action with workspace caching. The Linux row marked dependency-only uses dependency-only caches in both variants. These comparisons change the action and, for workspace-cache rows, the retained artifacts together.

Twenty paired runs per row. Available caches were seeded at `acba99d5` and restored at `7a8f63f0`, six first-parent commits later. The intervening changes mostly affect `ty`; Cargo manifests, lockfiles, and the toolchain are unchanged. Typing conformance builds the newer revision and then the older revision in each warm job. Both variants restored exact Rust-cache hits except the parser baseline, which has no matching producer and misses in every pair. No measured job uploaded a Rust cache. These intervals describe uncertainty in the mean for this revision pair, not variation across future source changes.

| Job / scope                                                                                   | Before → after (s) | Speedup |    Saved (s; 90% CI) |
| --------------------------------------------------------------------------------------------- | -----------------: | ------: | -------------------: |
| [Clippy](https://github.com/astral-sh/ruff/actions/runs/40138262778)                          |      121.8 → 100.6 |   1.21× |  21.2 (14.8 to 27.5) |
| [Linux tests (dependencies only)](https://github.com/astral-sh/ruff/actions/runs/40139576230) |      281.8 → 279.6 |   1.01× |    2.2 (-0.3 to 4.6) |
| [Linux release tests](https://github.com/astral-sh/ruff/actions/runs/40137674593)             |      184.5 → 171.1 |   1.08× |  13.4 (11.5 to 15.4) |
| [WebAssembly tests](https://github.com/astral-sh/ruff/actions/runs/40138264650)               |      148.4 → 118.1 |   1.26× |  30.2 (21.8 to 38.6) |
| [Fuzz target build](https://github.com/astral-sh/ruff/actions/runs/40138351629)               |      377.7 → 322.0 |   1.17× |  55.7 (37.0 to 74.4) |
| [Parser fuzzer†](https://github.com/astral-sh/ruff/actions/runs/40140050563)                  |       124.4 → 32.8 |   3.79× | 91.6 (83.0 to 100.2) |
| [Scripts](https://github.com/astral-sh/ruff/actions/runs/40138266390)                         |        72.5 → 64.0 |   1.13× |    8.5 (5.0 to 11.9) |
| [Ruff ecosystem†](https://github.com/astral-sh/ruff/actions/runs/40140209794)                 |        44.7 → 23.4 |   1.91× |  21.3 (19.3 to 23.4) |
| [`ty` completion evaluation](https://github.com/astral-sh/ruff/actions/runs/40137589773)      |      143.4 → 133.0 |   1.08× |   10.4 (7.3 to 13.4) |
| [Python package](https://github.com/astral-sh/ruff/actions/runs/40138153574)                  |        67.4 → 41.5 |   1.63× |  26.0 (23.0 to 28.9) |
| [Documentation](https://github.com/astral-sh/ruff/actions/runs/40138381886)                   |      143.7 → 114.6 |   1.25× |  29.2 (22.4 to 35.9) |
| [Formatter comparison](https://github.com/astral-sh/ruff/actions/runs/40138383678)            |      157.9 → 139.9 |   1.13× |   18.0 (6.9 to 29.1) |
| [Playground](https://github.com/astral-sh/ruff/actions/runs/40138607659)                      |      165.1 → 127.6 |   1.29× |  37.5 (27.8 to 47.1) |
| [Instrumented Ruff benchmark†](https://github.com/astral-sh/ruff/actions/runs/40140211369)    |       125.8 → 39.3 |   3.20× |  86.4 (83.6 to 89.3) |
| [Walltime benchmark build](https://github.com/astral-sh/ruff/actions/runs/40137759896)        |      183.5 → 175.0 |   1.05× |    8.4 (4.3 to 12.6) |
| [Typing conformance](https://github.com/astral-sh/ruff/actions/runs/40140551804)              |        81.5 → 75.0 |   1.09× |     6.4 (4.1 to 8.7) |

The measured after variants use `8139eb47` for root workspaces and `243f4258` for the nested fuzz build. The latter extends build-script input restoration to local path dependencies outside a nested workspace.

Cache lookup timeouts also affected two additional attempts: [typing with the shared action](https://github.com/astral-sh/ruff/actions/runs/40140702247/job/121011660117) reached its 10-minute job limit, and the [ecosystem baseline](https://github.com/astral-sh/ruff/actions/runs/40140514547/job/121011086521) exhausted retries after about 15½ minutes. Both failed on `GetCacheEntryDownloadURL` before downloading cache contents. Neither restored a cache; each comparison required 21 attempts to collect 20 successful warm pairs.

</details>

<details>
<summary>Earlier full workspace comparisons</summary>

Each row compares dependency-only caching with `Swatinem/rust-cache@6323deb1` against full-workspace caching with `rust-workspace-cache@8139eb47`, using the same source-change scenario as above:

| Job / scope                                                                                     | Before → after (s) | Speedup |     Saved (s; 90% CI) |
| ----------------------------------------------------------------------------------------------- | -----------------: | ------: | --------------------: |
| [Linux tests](https://github.com/astral-sh/ruff/actions/runs/40138593188)                       |      280.9 → 280.6 |   1.00× |     0.3 (-3.3 to 3.8) |
| [Windows tests](https://github.com/astral-sh/ruff/actions/runs/40138322371)                     |      244.7 → 250.8 |   0.98× |  -6.1 (-27.1 to 14.9) |
| [macOS tests](https://github.com/astral-sh/ruff/actions/runs/40136715210)                       |      186.2 → 196.5 |   0.95× | -10.3 (-12.9 to -7.8) |
| [Instrumented `ty` benchmark build](https://github.com/astral-sh/ruff/actions/runs/40138822816) |      142.3 → 139.9 |   1.02× |      2.5 (0.5 to 4.4) |
| [Daily Ruff fuzzer†](https://github.com/astral-sh/ruff/actions/runs/40139125815)                |        94.3 → 85.5 |   1.10× |     8.7 (4.5 to 13.0) |

For this revision pair, the Linux and Windows intervals include zero savings, and the macOS job is slower with full workspace caching. The instrumented `ty` benchmark adds about 513 MB for 2.5 seconds saved. The daily fuzzer adds about 407 MB for 8.7 seconds of build savings within a fixed 20-minute budget.

</details>

<details>
<summary>Other migrated jobs</summary>

These comparisons use the measured source revision, with 20 pairs per row.

Dependency-only caches (seeded from that revision; exact hits in both variants):

| Job / scope                                                                                     | Before → after (s) | Speedup |    Saved (s; 90% CI) |
| ----------------------------------------------------------------------------------------------- | -----------------: | ------: | -------------------: |
| [Hawk](https://github.com/astral-sh/ruff/actions/runs/40123536856)                              |      350.4 → 337.5 |   1.04× | 12.8 (-11.9 to 37.6) |
| [Publish dry-run](https://github.com/astral-sh/ruff/actions/runs/40129317143)                   |      198.0 → 200.7 |   0.99× |  -2.7 (-5.2 to -0.2) |
| [Windows tests](https://github.com/astral-sh/ruff/actions/runs/40130850769)                     |      243.8 → 236.7 |   1.03× |   7.2 (-9.6 to 23.9) |
| [macOS tests](https://github.com/astral-sh/ruff/actions/runs/40130742599)                       |      182.8 → 183.3 |   1.00× |   -0.5 (-3.3 to 2.3) |
| [MSRV build](https://github.com/astral-sh/ruff/actions/runs/40129396768)                        |        87.9 → 82.4 |   1.07× |    5.5 (0.9 to 10.1) |
| [Instrumented `ty` benchmark build](https://github.com/astral-sh/ruff/actions/runs/40130744879) |      144.1 → 140.9 |   1.02× |     3.2 (1.3 to 5.1) |
| [Daily Ruff fuzzer†](https://github.com/astral-sh/ruff/actions/runs/40130746641)                |        94.3 → 92.8 |   1.02× |    1.6 (-4.8 to 7.9) |

Jobs without a cache producer (both variants miss in all 20 pairs):

| Job / scope                                                                              | Before → after (s) | Speedup |  Saved (s; 90% CI) |
| ---------------------------------------------------------------------------------------- | -----------------: | ------: | -----------------: |
| [`ty` panic fuzzer†](https://github.com/astral-sh/ruff/actions/runs/40130395250)         |        92.6 → 90.9 |   1.02× |   1.7 (0.6 to 2.8) |
| [Memory report†](https://github.com/astral-sh/ruff/actions/runs/40130397120)             |        97.4 → 97.7 |   1.00× | -0.3 (-4.6 to 3.9) |
| [`ty` ecosystem: base build](https://github.com/astral-sh/ruff/actions/runs/40118572694) |        94.4 → 92.3 |   1.02× |  2.1 (-3.3 to 7.5) |
| [`ty` ecosystem: PR build](https://github.com/astral-sh/ruff/actions/runs/40118575076)   |        94.8 → 89.8 |   1.06× |  5.0 (0.1 to 10.0) |

The `main` producer jobs average 18.3 s (90% CI 17.7 to 19.0) on Depot and 24.6 s (90% CI 23.1 to 26.0) on GitHub, with 20 exact-hit runs each. These have no before variant.

PGO averages 71.0 s (90% CI 70.5 to 71.6) with the native cache (20 warm runs). No speedup is reported: the [warmed baseline](https://github.com/astral-sh/ruff/actions/runs/40118437851) fails its cached-checkout origin check.

Workflow baseline: `ea186584`; both variants use `uv` `0.13.5` where installed. Source: `7a8f63f0`; publish uses `30be8844`. Before: `rust-cache@6323deb1`. The linked workflows record the measured action revisions. Namespace Windows durations use GitHub job start/end timestamps because runner log clocks can jump; other rows use runner-log timestamps.

</details>
