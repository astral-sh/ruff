# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.28.2](https://github.com/salsa-rs/salsa/compare/salsa-v0.28.1...salsa-v0.28.2) - 2026-08-01

### Added

- Add conditional SalsaValue derive proofs ([#1261](https://github.com/salsa-rs/salsa/pull/1261))

### Fixed

- propagate poisoned cycle heads ([#1263](https://github.com/salsa-rs/salsa/pull/1263))
- migrate procedural macros to syn 3 ([#1265](https://github.com/salsa-rs/salsa/pull/1265))

### Other

- reduce function ingredient monomorphization ([#1210](https://github.com/salsa-rs/salsa/pull/1210))
- clarify ingredient cache safety invariants ([#1270](https://github.com/salsa-rs/salsa/pull/1270))
- reduce specified value validation monomorphization ([#1209](https://github.com/salsa-rs/salsa/pull/1209))

## [0.28.1](https://github.com/salsa-rs/salsa/compare/salsa-v0.28.0...salsa-v0.28.1) - 2026-07-22

### Other

- avoid table lookups when interning values ([#1192](https://github.com/salsa-rs/salsa/pull/1192))
- Require immortal storage for lifetime-free interned values ([#1259](https://github.com/salsa-rs/salsa/pull/1259))

## [0.28.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.27.2...salsa-v0.28.0) - 2026-07-12

### Added

- replace `Update` with `SalsaValue` and `PartialEq` ([#1217](https://github.com/salsa-rs/salsa/pull/1217))
- return references by default ([#1216](https://github.com/salsa-rs/salsa/pull/1216))
- report per-ingredient page occupancy ([#1181](https://github.com/salsa-rs/salsa/pull/1181))

### Fixed

- restore compact interned value layout ([#1244](https://github.com/salsa-rs/salsa/pull/1244))
- implement SalsaValue for FxBuildHasher ([#1241](https://github.com/salsa-rs/salsa/pull/1241))
- restart fixpoint iteration after cancellation ([#1238](https://github.com/salsa-rs/salsa/pull/1238))
- replace tracked method db-arg index panic with proper compile error ([#1228](https://github.com/salsa-rs/salsa/pull/1228))

### Other

- reduce memo validation monomorphization ([#1206](https://github.com/salsa-rs/salsa/pull/1206))
- reduce interned constructor monomorphization ([#1208](https://github.com/salsa-rs/salsa/pull/1208))
- avoid hashing query edge generations ([#1223](https://github.com/salsa-rs/salsa/pull/1223))
- erase configuration from interned storage ([#1204](https://github.com/salsa-rs/salsa/pull/1204))
- reduce table page size ([#1179](https://github.com/salsa-rs/salsa/pull/1179))
- Add untracked field read benchmark ([#1225](https://github.com/salsa-rs/salsa/pull/1225))
- Revise salsa benchmarks ([#1224](https://github.com/salsa-rs/salsa/pull/1224))
- update Salsa book for current APIs ([#1215](https://github.com/salsa-rs/salsa/pull/1215))
- document proc macros and Salsa concepts ([#1213](https://github.com/salsa-rs/salsa/pull/1213))
- simplify specify origin handling ([#1214](https://github.com/salsa-rs/salsa/pull/1214))
- reduce table monomorphization ([#1211](https://github.com/salsa-rs/salsa/pull/1211))
- reduce ingredient cache monomorphization ([#1207](https://github.com/salsa-rs/salsa/pull/1207))
- inline tracked struct revision loads ([#1202](https://github.com/salsa-rs/salsa/pull/1202))
- avoid repeated cycle dependency lookups ([#1201](https://github.com/salsa-rs/salsa/pull/1201))

## [0.27.2](https://github.com/salsa-rs/salsa/compare/salsa-v0.27.1...salsa-v0.27.2) - 2026-06-25

### Fixed

- propagate cycle heads through specify ([#1190](https://github.com/salsa-rs/salsa/pull/1190))
- seed cycle disambiguators once ([#1191](https://github.com/salsa-rs/salsa/pull/1191))
- omit never-change input edges ([#1193](https://github.com/salsa-rs/salsa/pull/1193))

## [0.27.1](https://github.com/salsa-rs/salsa/compare/salsa-v0.27.0...salsa-v0.27.1) - 2026-06-24

### Added

- make detailed tracing opt-in ([#1188](https://github.com/salsa-rs/salsa/pull/1188))

### Changed

- Update Rust crate hashlink to 0.12 ([#1131](https://github.com/salsa-rs/salsa/pull/1131))

### Fixed

- preserve current query values when specifying ([#1182](https://github.com/salsa-rs/salsa/pull/1182))
- make interned slot reuse unwind-safe ([#1160](https://github.com/salsa-rs/salsa/pull/1160))
- record each interned revision once ([#1163](https://github.com/salsa-rs/salsa/pull/1163))
- preserve query revision for non-reusable interned values ([#1180](https://github.com/salsa-rs/salsa/pull/1180))
- hold shard lock while reading interned entries ([#1159](https://github.com/salsa-rs/salsa/pull/1159))
- discard tracked outputs overwritten by specify ([#1162](https://github.com/salsa-rs/salsa/pull/1162))
- exclude deleted tracked structs from enumeration ([#1166](https://github.com/salsa-rs/salsa/pull/1166))
- reset cancellation after event panic ([#1157](https://github.com/salsa-rs/salsa/pull/1157))
- guarantee RawDatabase ABI transparency ([#1165](https://github.com/salsa-rs/salsa/pull/1165))
- restore cancellation state after cycle panic ([#1156](https://github.com/salsa-rs/salsa/pull/1156))
- preserve accumulators during shallow verification ([#1155](https://github.com/salsa-rs/salsa/pull/1155))
- support updating empty arrays ([#1153](https://github.com/salsa-rs/salsa/pull/1153))
- reject singleton on non-input structs ([#1154](https://github.com/salsa-rs/salsa/pull/1154))
- preserve order when updating OrderMap ([#1152](https://github.com/salsa-rs/salsa/pull/1152))
- reject zero atomic revisions during deserialization ([#1151](https://github.com/salsa-rs/salsa/pull/1151))
- fix default interned GC configuration ([#1146](https://github.com/salsa-rs/salsa/pull/1146))
- *(book)* update embedded diagrams.net source ([#1149](https://github.com/salsa-rs/salsa/pull/1149))
- avoid redundant captures in input setters ([#1144](https://github.com/salsa-rs/salsa/pull/1144))

### Other

- allocate memo table slots lazily ([#1178](https://github.com/salsa-rs/salsa/pull/1178))
- Add never-change durability and omit immutable query edges ([#1109](https://github.com/salsa-rs/salsa/pull/1109))
- separate cached and cold fetch paths ([#1184](https://github.com/salsa-rs/salsa/pull/1184))
- Introduce `MemoHeader` to reduce monomorphization ([#1142](https://github.com/salsa-rs/salsa/pull/1142))
- store interned revisions inline ([#1175](https://github.com/salsa-rs/salsa/pull/1175))
- skip empty cycle-head propagation ([#1176](https://github.com/salsa-rs/salsa/pull/1176))
- Improve ergonomics of `Update` derive helpers ([#1168](https://github.com/salsa-rs/salsa/pull/1168))
- Fix triomphe typos ([#1170](https://github.com/salsa-rs/salsa/pull/1170))
- Revamp tracked attribute for methods and impls to better handle lifetimes ([#1111](https://github.com/salsa-rs/salsa/pull/1111))
- fix typos ([#1132](https://github.com/salsa-rs/salsa/pull/1132))
- Remove remaining mentions of `maybe_changed_since` ([#1145](https://github.com/salsa-rs/salsa/pull/1145))
- Skip dependencies on non-reusable interned values ([#1108](https://github.com/salsa-rs/salsa/pull/1108))
- separate LRU reads from collection ([#1138](https://github.com/salsa-rs/salsa/pull/1138))
- LRU benches ([#1110](https://github.com/salsa-rs/salsa/pull/1110))
- Avoid Miri cache save races ([#1125](https://github.com/salsa-rs/salsa/pull/1125))
- Use single allocation to store origin and `QueryRevisionExtra` ([#1112](https://github.com/salsa-rs/salsa/pull/1112))
- Shard Miri tests ([#1123](https://github.com/salsa-rs/salsa/pull/1123))
- Update actions/upload-pages-artifact action to v5 ([#1122](https://github.com/salsa-rs/salsa/pull/1122))
- Update actions/configure-pages action to v6 ([#1120](https://github.com/salsa-rs/salsa/pull/1120))
- Update actions/deploy-pages action to v5 ([#1121](https://github.com/salsa-rs/salsa/pull/1121))
- Pin the stable Rust toolchain ([#1119](https://github.com/salsa-rs/salsa/pull/1119))
- Update taiki-e/install-action action to v2.81.3 ([#1117](https://github.com/salsa-rs/salsa/pull/1117))
- Pin dependencies ([#1116](https://github.com/salsa-rs/salsa/pull/1116))
- Configure Renovate dependency updates ([#1114](https://github.com/salsa-rs/salsa/pull/1114))
- Reduce metadata per memo by packing `QueryRevisionsExtra` into `QueryOrigin` ([#1101](https://github.com/salsa-rs/salsa/pull/1101))
- Add packed `QueryOrigin` ([#1103](https://github.com/salsa-rs/salsa/pull/1103))

## [0.27.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.26.2...salsa-v0.27.0) - 2026-06-04

### Fixed

- annotate return type in closure ([#1106](https://github.com/salsa-rs/salsa/pull/1106))
- accumulated values lost when reused tracked fn skips re-execution ([#1099](https://github.com/salsa-rs/salsa/pull/1099))

### Other

- Invalidate fixpoint memos after cancellation ([#1100](https://github.com/salsa-rs/salsa/pull/1100))
- Update dependencies ([#1105](https://github.com/salsa-rs/salsa/pull/1105))
- Refresh trybuild snapshots for Rust 1.96 ([#1102](https://github.com/salsa-rs/salsa/pull/1102))
- Remove outdated RELEASES.md file; exclude a few more files from published crates ([#1091](https://github.com/salsa-rs/salsa/pull/1091))
- Skip memory usage tests on 32-bit (non-64-bit) targets ([#1094](https://github.com/salsa-rs/salsa/pull/1094))
- Convert CRLF-terminated sources to LF-terminated ([#1092](https://github.com/salsa-rs/salsa/pull/1092))

## [0.26.2](https://github.com/salsa-rs/salsa/compare/salsa-v0.26.1...salsa-v0.26.2) - 2026-05-03

### Fixed

- fix tracked impl db lifetime for as_deref return mode ([#1084](https://github.com/salsa-rs/salsa/pull/1084))

### Other

- Do not use `#[allow(non_local_definitions)]` ([#1090](https://github.com/salsa-rs/salsa/pull/1090))
- Exclude the book from published crates ([#1089](https://github.com/salsa-rs/salsa/pull/1089))
- Detect overlapping supertype variants via leaf type IDs ([#1080](https://github.com/salsa-rs/salsa/pull/1080))
- Bump hashbrown to 0.17 ([#1087](https://github.com/salsa-rs/salsa/pull/1087))
- Expose `Revision::max()` and make it const ([#1086](https://github.com/salsa-rs/salsa/pull/1086))

## [0.26.1](https://github.com/salsa-rs/salsa/compare/salsa-v0.26.0...salsa-v0.26.1) - 2026-03-20

### Other

- Bump the edition to 2024 ([#1073](https://github.com/salsa-rs/salsa/pull/1073))
- Don't panic if a query branched on an untracked state ([#1075](https://github.com/salsa-rs/salsa/pull/1075))

## [0.26.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.25.2...salsa-v0.26.0) - 2026-02-02

### Added

- Allow opt-ing out of LRU at compile time ([#1051](https://github.com/salsa-rs/salsa/pull/1051))
- Allow tracked function cycle attributes to take closures ([#1048](https://github.com/salsa-rs/salsa/pull/1048))

### Fixed

- Do not alias fields of `tracked_struct` `Value`s when updating ([#741](https://github.com/salsa-rs/salsa/pull/741))

### Other

- Remove unnecessary `boxcar::Vec` ([#1072](https://github.com/salsa-rs/salsa/pull/1072))
- replace remaining instances of `cycle_fallback` with `cycle_result` in the docs ([#1071](https://github.com/salsa-rs/salsa/pull/1071))
- Fix out-of-order verification of cycle head dependencies ([#1061](https://github.com/salsa-rs/salsa/pull/1061))
- Update compile fail tests ([#1070](https://github.com/salsa-rs/salsa/pull/1070))
- Merge `FallbackImmediate` and `Fixpoint` code paths ([#1063](https://github.com/salsa-rs/salsa/pull/1063))
- Introduce a `CancellationToken` for cancelling specific computations ([#1007](https://github.com/salsa-rs/salsa/pull/1007))
- Remove unnecessary backdate field macros ([#1069](https://github.com/salsa-rs/salsa/pull/1069))
- Remove `ValueWithMetadata` ([#1057](https://github.com/salsa-rs/salsa/pull/1057))
- Fix stale tracked struct values in later iterations ([#1068](https://github.com/salsa-rs/salsa/pull/1068))
- Skip book deployment on forks ([#1066](https://github.com/salsa-rs/salsa/pull/1066))
- Remove `QueryOriginKind::FixpointInitial` ([#1062](https://github.com/salsa-rs/salsa/pull/1062))
- Document cycle_fallback in the book ([#1056](https://github.com/salsa-rs/salsa/pull/1056))
- Fix book build with newer mdbook versions ([#1055](https://github.com/salsa-rs/salsa/pull/1055))
- Implement `Lookup` and `HashEqLike` for `Cow` ([#1054](https://github.com/salsa-rs/salsa/pull/1054))
- Add `DidFinalizeCycle` event, add more tests for cyclic nested queries ([#1052](https://github.com/salsa-rs/salsa/pull/1052))
- Collect cycle heads transitively ([#1050](https://github.com/salsa-rs/salsa/pull/1050))
- Remove unused database forking ([#1049](https://github.com/salsa-rs/salsa/pull/1049))
- Reduce monomorphized code in maybe_changed_after ([#1047](https://github.com/salsa-rs/salsa/pull/1047))
- Reduce monomorphized code in `execute_maybe_iterate` ([#1046](https://github.com/salsa-rs/salsa/pull/1046))
- Add `salsa::Update` trait bounds to generics when deriving `salsa::Update` ([#1041](https://github.com/salsa-rs/salsa/pull/1041))

## [0.25.2](https://github.com/salsa-rs/salsa/compare/salsa-v0.25.1...salsa-v0.25.2) - 2025-12-17

### Other

- Revert #958 ([#1039](https://github.com/salsa-rs/salsa/pull/1039))

## [0.25.1](https://github.com/salsa-rs/salsa/compare/salsa-v0.25.0...salsa-v0.25.1) - 2025-12-16

### Other

- Fix a remainder that was forgotten in #1036 ([#1037](https://github.com/salsa-rs/salsa/pull/1037))

## [0.25.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.24.0...salsa-v0.25.0) - 2025-12-16

### Other

- Require interned structs' fields to be `Update` ([#1036](https://github.com/salsa-rs/salsa/pull/1036))
- Make `ordermap` an optional feature ([#1034](https://github.com/salsa-rs/salsa/pull/1034))
- implement `Update` for `OrderMap` and `OrderSet` ([#1033](https://github.com/salsa-rs/salsa/pull/1033))
- Fully qualify std Result type ([#1025](https://github.com/salsa-rs/salsa/pull/1025))
- pass `Cycle` to the cycle recovery function ([#1028](https://github.com/salsa-rs/salsa/pull/1028))
- Fix cycle head durability ([#1024](https://github.com/salsa-rs/salsa/pull/1024))
- Call `cycle_fn` for every iteration ([#1021](https://github.com/salsa-rs/salsa/pull/1021))
- Track cycle function dependencies as part of the cyclic query ([#1018](https://github.com/salsa-rs/salsa/pull/1018))
- Always increment iteration count ([#1017](https://github.com/salsa-rs/salsa/pull/1017))
- Update compile fail snapshots to match new rust stable output ([#1020](https://github.com/salsa-rs/salsa/pull/1020))
- Only use provisional values from the same revision ([#1019](https://github.com/salsa-rs/salsa/pull/1019))
- Explain the motivation for breaking API changes made in #1012 and #1015 ([#1016](https://github.com/salsa-rs/salsa/pull/1016))
- Expose the Input query Id with  cycle_initial ([#1015](https://github.com/salsa-rs/salsa/pull/1015))
- Add `SyncTable::peek_claim` fast path for `function::Ingredient::wait_for` ([#1011](https://github.com/salsa-rs/salsa/pull/1011))
- Fix cache invalidation when cycle head becomes non-head ([#1014](https://github.com/salsa-rs/salsa/pull/1014))
- Expose the query ID and the last provisional value to the cycle recovery function ([#1012](https://github.com/salsa-rs/salsa/pull/1012))
- Fix hangs in multithreaded fixpoint iteration ([#1010](https://github.com/salsa-rs/salsa/pull/1010))
- Remove experimental parallel feature ([#1013](https://github.com/salsa-rs/salsa/pull/1013))
- Simplify `WaitGroup` implementation  ([#958](https://github.com/salsa-rs/salsa/pull/958))
- Fix missing license files in published macros/macro-rules crates ([#1009](https://github.com/salsa-rs/salsa/pull/1009))
- Run fixpoint per strongly connected component ([#999](https://github.com/salsa-rs/salsa/pull/999))
- Add benchmark for a fixpoint iteration with nested cycles ([#1001](https://github.com/salsa-rs/salsa/pull/1001))

## [0.24.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.23.0...salsa-v0.24.0) - 2025-09-30

### Fixed

- Cleanup provisional cycle head memos when query panics ([#993](https://github.com/salsa-rs/salsa/pull/993))
- Runaway for unchanged queries participating in cycle ([#981](https://github.com/salsa-rs/salsa/pull/981))
- Delete not re-created tracked structs after fixpoint iteration ([#979](https://github.com/salsa-rs/salsa/pull/979))
- fix assertion during interned deserialization ([#978](https://github.com/salsa-rs/salsa/pull/978))
- Do not unnecessarily require `Debug` on fields for interned structs ([#951](https://github.com/salsa-rs/salsa/pull/951))
- Fix phantom data usage in salsa structs affecting auto traits ([#932](https://github.com/salsa-rs/salsa/pull/932))

### Other

- Replace unsafe unwrap with `expect` call ([#998](https://github.com/salsa-rs/salsa/pull/998))
- Push active query in execute ([#996](https://github.com/salsa-rs/salsa/pull/996))
- Update codspeed action ([#997](https://github.com/salsa-rs/salsa/pull/997))
- Add implementations for Lookup and HashEqLike for CompactString ([#988](https://github.com/salsa-rs/salsa/pull/988))
- Provide a method to attach a database even if it's different from the current attached one ([#992](https://github.com/salsa-rs/salsa/pull/992))
- Allow fallback to take longer than one iteration to converge ([#991](https://github.com/salsa-rs/salsa/pull/991))
- refactor `entries` API ([#987](https://github.com/salsa-rs/salsa/pull/987))
- Persistent caching fixes ([#982](https://github.com/salsa-rs/salsa/pull/982))
- outline cold path of `lookup_ingredient` ([#984](https://github.com/salsa-rs/salsa/pull/984))
- Update snapshot to fix nightly type rendering ([#983](https://github.com/salsa-rs/salsa/pull/983))
- avoid cycles during serialization ([#977](https://github.com/salsa-rs/salsa/pull/977))
- Flatten unserializable query dependencies ([#975](https://github.com/salsa-rs/salsa/pull/975))
- optimize `Id::hash` ([#974](https://github.com/salsa-rs/salsa/pull/974))
- Make `thin-vec/serde` dependency dependent on `persistence` feature ([#973](https://github.com/salsa-rs/salsa/pull/973))
- Remove tracked structs from query outputs ([#969](https://github.com/salsa-rs/salsa/pull/969))
- Remove jemalloc ([#972](https://github.com/salsa-rs/salsa/pull/972))
- Initial persistent caching prototype ([#967](https://github.com/salsa-rs/salsa/pull/967))
- Fix `maybe_changed_after` runnaway for fixpoint queries ([#961](https://github.com/salsa-rs/salsa/pull/961))
- add parallel maybe changed after test ([#963](https://github.com/salsa-rs/salsa/pull/963))
- Update tests for Rust 1.89 ([#966](https://github.com/salsa-rs/salsa/pull/966))
- remove allocation lock ([#962](https://github.com/salsa-rs/salsa/pull/962))
- consolidate memory usage information API ([#964](https://github.com/salsa-rs/salsa/pull/964))
- Add heap size support for salsa structs ([#943](https://github.com/salsa-rs/salsa/pull/943))
- Extract the cycle branches from `fetch` and `maybe_changed_after` ([#955](https://github.com/salsa-rs/salsa/pull/955))
- allow reuse of cached provisional memos within the same cycle iteration during `maybe_changed_after` ([#954](https://github.com/salsa-rs/salsa/pull/954))
- Expose API to manually trigger cancellation ([#959](https://github.com/salsa-rs/salsa/pull/959))
- Upgrade dependencies ([#956](https://github.com/salsa-rs/salsa/pull/956))
- Use `CycleHeadSet` in `maybe_update_after` ([#953](https://github.com/salsa-rs/salsa/pull/953))
- Gate accumulator feature behind a feature flag ([#946](https://github.com/salsa-rs/salsa/pull/946))
- optimize allocation fast-path ([#949](https://github.com/salsa-rs/salsa/pull/949))
- remove borrow checks from `ZalsaLocal` ([#939](https://github.com/salsa-rs/salsa/pull/939))
- Do manual trait casting ([#922](https://github.com/salsa-rs/salsa/pull/922))
- Retain backing allocation of `ActiveQuery::input_outputs` in `ActiveQuery::seed_iteration` ([#948](https://github.com/salsa-rs/salsa/pull/948))
- remove extra bounds checks from memo table hot-paths ([#938](https://github.com/salsa-rs/salsa/pull/938))
- Outline all tracing events ([#942](https://github.com/salsa-rs/salsa/pull/942))
- remove bounds and type checks from `IngredientCache` ([#937](https://github.com/salsa-rs/salsa/pull/937))
- Avoid dynamic dispatch to access memo tables ([#941](https://github.com/salsa-rs/salsa/pull/941))
- optimize page access ([#940](https://github.com/salsa-rs/salsa/pull/940))
- Use `inventory` for static ingredient registration ([#934](https://github.com/salsa-rs/salsa/pull/934))
- Fix `heap_size` option not being preserved in tracked impls ([#930](https://github.com/salsa-rs/salsa/pull/930))
- update papaya ([#928](https://github.com/salsa-rs/salsa/pull/928))

## [0.23.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.22.0...salsa-v0.23.0) - 2025-06-27

### Added

- `Update` derive field overwrite support ([#747](https://github.com/salsa-rs/salsa/pull/747))

### Fixed

- fix race in `MemoTableTypes` ([#912](https://github.com/salsa-rs/salsa/pull/912))
- multithreaded nested fixpoint iteration ([#882](https://github.com/salsa-rs/salsa/pull/882))

### Other

- Emit self ty for query debug name of assoc function queries ([#927](https://github.com/salsa-rs/salsa/pull/927))
- Replace ingredient cache with faster ingredient map ([#921](https://github.com/salsa-rs/salsa/pull/921))
- add option to track heap memory usage of memos ([#925](https://github.com/salsa-rs/salsa/pull/925))
- Hide generated structs of tracked functions from docs via `#[doc(hidden)]` ([#917](https://github.com/salsa-rs/salsa/pull/917))
- Add API to dump memory usage ([#916](https://github.com/salsa-rs/salsa/pull/916))
- Revert "Assert size for interned Value" & Mark `Slot` trait as unsafe ([#915](https://github.com/salsa-rs/salsa/pull/915))
- add an option to tune interned garbage collection ([#911](https://github.com/salsa-rs/salsa/pull/911))
- Use explicit discriminants for `QueryOriginKind` for better comparisons ([#913](https://github.com/salsa-rs/salsa/pull/913))
- update boxcar ([#910](https://github.com/salsa-rs/salsa/pull/910))
- use latest revision for dependencies on interned values ([#908](https://github.com/salsa-rs/salsa/pull/908))
- remove high-durability values from interned LRU ([#907](https://github.com/salsa-rs/salsa/pull/907))
- Preserve attributes on interned/tracked struct fields ([#905](https://github.com/salsa-rs/salsa/pull/905))
- Assert size for interned `Value` ([#901](https://github.com/salsa-rs/salsa/pull/901))
- reduce size of interned value metadata ([#903](https://github.com/salsa-rs/salsa/pull/903))
- panic with string message again for cycle panics ([#898](https://github.com/salsa-rs/salsa/pull/898))
- Use `Revision` and `Durability` directly in input `Value` ([#902](https://github.com/salsa-rs/salsa/pull/902))
- Fix flaky parallel_join test ([#900](https://github.com/salsa-rs/salsa/pull/900))
- Bump MSRV to 1.85 ([#899](https://github.com/salsa-rs/salsa/pull/899))
- Simple LRU garbage collection for interned values ([#839](https://github.com/salsa-rs/salsa/pull/839))
- Capture execution backtrace when throwing `UnexpectedCycle` ([#883](https://github.com/salsa-rs/salsa/pull/883))
- Store tracked struct ids as ThinVec on Revisions ([#892](https://github.com/salsa-rs/salsa/pull/892))
- Update dependencies, remove unused `heck` dependency ([#894](https://github.com/salsa-rs/salsa/pull/894))
- Set `validate_final` in `execute` after removing the last cycle head ([#890](https://github.com/salsa-rs/salsa/pull/890))
- Pack `QueryEdge` memory layout ([#886](https://github.com/salsa-rs/salsa/pull/886))
- Lazily allocate extra memo state ([#888](https://github.com/salsa-rs/salsa/pull/888))
- Pack `QueryOrigin` memory layout ([#885](https://github.com/salsa-rs/salsa/pull/885))
- Restrict memo size assertion to 64bit platforms ([#884](https://github.com/salsa-rs/salsa/pull/884))
- Don't report stale outputs if there is newer generation in new_outputs ([#879](https://github.com/salsa-rs/salsa/pull/879))
- Fix hang in nested fixpoint iteration ([#871](https://github.com/salsa-rs/salsa/pull/871))
- Add debug spans for `new_revision` and `evict_lru` ([#881](https://github.com/salsa-rs/salsa/pull/881))
- Add fetch span ([#875](https://github.com/salsa-rs/salsa/pull/875))
- shrink_to_fit `IdentityMap` before storing it ([#816](https://github.com/salsa-rs/salsa/pull/816))
- Allow lifetimes in arguments in tracked fns with >1 parameters ([#880](https://github.com/salsa-rs/salsa/pull/880))
- Replace loom with shuttle ([#876](https://github.com/salsa-rs/salsa/pull/876))
- Use generational identifiers for tracked structs ([#864](https://github.com/salsa-rs/salsa/pull/864))

### Fixed

- `#[doc(hidden)]` auto-generated tracked-fn structs ([#917](https://github.com/salsa-rs/salsa/pull/917))

## [0.22.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.21.1...salsa-v0.22.0) - 2025-05-23

### Fixed

- fix memo table growth condition ([#850](https://github.com/salsa-rs/salsa/pull/850))
- incorrect caching for queries participating in fixpoint ([#843](https://github.com/salsa-rs/salsa/pull/843))
- change detection for fixpoint queries ([#836](https://github.com/salsa-rs/salsa/pull/836))

### Other

- Allow creation of tracked associated functions (without `self`) ([#859](https://github.com/salsa-rs/salsa/pull/859))
- Short-circuit `block-on` if same thread ([#862](https://github.com/salsa-rs/salsa/pull/862))
- Skip release-plz jobs on forks ([#873](https://github.com/salsa-rs/salsa/pull/873))
- Unwind with specific type when encountering an unexpected cycle ([#856](https://github.com/salsa-rs/salsa/pull/856))
- Remove jar mentions from book ([#775](https://github.com/salsa-rs/salsa/pull/775))
- Implement an `!Update` bound escape hatch for tracked fn ([#867](https://github.com/salsa-rs/salsa/pull/867))
- Only enable `boxcar/loom` when `loom` feature is enabled ([#869](https://github.com/salsa-rs/salsa/pull/869))
- Remove default `PartialOrd` and `Ord` derives for salsa-structs ([#868](https://github.com/salsa-rs/salsa/pull/868))
- update boxcar ([#865](https://github.com/salsa-rs/salsa/pull/865))
- speed-up cycle-retry logic ([#861](https://github.com/salsa-rs/salsa/pull/861))
- Fix returns(deref | as_ref | as_deref) in tracked methods ([#857](https://github.com/salsa-rs/salsa/pull/857))
- Changed `return_ref` syntax to `returns(as_ref)` and `returns(cloned)` ([#772](https://github.com/salsa-rs/salsa/pull/772))
- Work around a rust-analyzer bug ([#855](https://github.com/salsa-rs/salsa/pull/855))
- Lazy finalization of cycle participants in `maybe_changed_after` ([#854](https://github.com/salsa-rs/salsa/pull/854))
- Do not re-verify already verified memoized value in cycle verification ([#851](https://github.com/salsa-rs/salsa/pull/851))
- Pass cycle heads as out parameter for `maybe_changed_after` ([#852](https://github.com/salsa-rs/salsa/pull/852))
- Move salsa event system into `Zalsa` ([#849](https://github.com/salsa-rs/salsa/pull/849))
- gate loom dependency under feature flag ([#844](https://github.com/salsa-rs/salsa/pull/844))
- Add loom support ([#842](https://github.com/salsa-rs/salsa/pull/842))
- Clean up some unsafety ([#830](https://github.com/salsa-rs/salsa/pull/830))

## [0.21.1](https://github.com/salsa-rs/salsa/compare/salsa-v0.21.0...salsa-v0.21.1) - 2025-04-30

### Added

- Make `attach` pub ([#832](https://github.com/salsa-rs/salsa/pull/832))

### Other

- better debug name for interned query arguments ([#837](https://github.com/salsa-rs/salsa/pull/837))
- Avoid panic in `Backtrace::capture` if `query_stack` is already borrowed ([#835](https://github.com/salsa-rs/salsa/pull/835))
- Clean up `function::execute` ([#833](https://github.com/salsa-rs/salsa/pull/833))
- Change an `assert!` to `assert_eq!` ([#828](https://github.com/salsa-rs/salsa/pull/828))

## [0.21.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.20.0...salsa-v0.21.0) - 2025-04-29

### Fixed

- Access to tracked-struct that was freed during fixpoint ([#817](https://github.com/salsa-rs/salsa/pull/817))
- correct debug output for tracked fields ([#826](https://github.com/salsa-rs/salsa/pull/826))
- Fix incorrect `values_equal` signature ([#825](https://github.com/salsa-rs/salsa/pull/825))
- allow unused lifetimes in tracked_struct expansion ([#824](https://github.com/salsa-rs/salsa/pull/824))

### Other

- Implement a query stack `Backtrace` analog ([#827](https://github.com/salsa-rs/salsa/pull/827))
- Simplify ID conversions ([#822](https://github.com/salsa-rs/salsa/pull/822))
- Attempt to fix codspeed ([#823](https://github.com/salsa-rs/salsa/pull/823))
- Remove unnecessary `Array` abstraction ([#821](https://github.com/salsa-rs/salsa/pull/821))
- Add a compile-fail test for a `'static` `!Update` struct ([#820](https://github.com/salsa-rs/salsa/pull/820))
- squelch most clippy warnings in generated code ([#809](https://github.com/salsa-rs/salsa/pull/809))
- Include struct name in formatted input-field index ([#819](https://github.com/salsa-rs/salsa/pull/819))
- Force inline `fetch_hot` ([#818](https://github.com/salsa-rs/salsa/pull/818))
- Per ingredient sync table ([#650](https://github.com/salsa-rs/salsa/pull/650))
- Use `DatabaseKey` for interned events ([#813](https://github.com/salsa-rs/salsa/pull/813))
- [refactor] More `fetch_hot` simplification ([#793](https://github.com/salsa-rs/salsa/pull/793))
- Don't store the fields in the interned map ([#812](https://github.com/salsa-rs/salsa/pull/812))
- Fix ci not always running ([#810](https://github.com/salsa-rs/salsa/pull/810))

## [0.20.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.19.0...salsa-v0.20.0) - 2025-04-22

### Added

- Drop `Debug` requirements and flip implementation defaults ([#756](https://github.com/salsa-rs/salsa/pull/756))

### Fixed

- Dereferencing freed memos when verifying provisional memos ([#788](https://github.com/salsa-rs/salsa/pull/788))
- `#[doc(hidden)]` `plumbing` module ([#781](https://github.com/salsa-rs/salsa/pull/781))
- Use `changed_at` revision when updating fields ([#778](https://github.com/salsa-rs/salsa/pull/778))

### Other

- Reduce memory usage by deduplicating type information ([#803](https://github.com/salsa-rs/salsa/pull/803))
- Make interned's `last_interned_at` equal `Revision::MAX` if they are interned outside a query ([#804](https://github.com/salsa-rs/salsa/pull/804))
- Add a third cycle mode, equivalent to old Salsa cycle behavior ([#801](https://github.com/salsa-rs/salsa/pull/801))
- Update compact_str from 0.8 to 0.9 ([#794](https://github.com/salsa-rs/salsa/pull/794))
- Implement `Update` for `ThinVec` ([#807](https://github.com/salsa-rs/salsa/pull/807))
- Don't push an unnecessary active query for `deep_verify_memo` ([#806](https://github.com/salsa-rs/salsa/pull/806))
- Inline/Outline more cold and slow paths ([#805](https://github.com/salsa-rs/salsa/pull/805))
- `#[inline]` some things ([#799](https://github.com/salsa-rs/salsa/pull/799))
- Discard unnecessary atomic load ([#780](https://github.com/salsa-rs/salsa/pull/780))
- Print query stack when encountering unexpected cycle ([#796](https://github.com/salsa-rs/salsa/pull/796))
- Remove incorrect `parallel_scope` API ([#797](https://github.com/salsa-rs/salsa/pull/797))
- [refactor] Simplify `fetch_hot` ([#792](https://github.com/salsa-rs/salsa/pull/792))
- [refactor] Reuse the same stack for all cycles heads in `validate_same_iteration` ([#791](https://github.com/salsa-rs/salsa/pull/791))
- add WillIterateCycle event ([#790](https://github.com/salsa-rs/salsa/pull/790))
- [fix] Use `validate_maybe_provisional` instead of `validate_provisional` ([#789](https://github.com/salsa-rs/salsa/pull/789))
- Use `ThinVec` for `CycleHeads` ([#787](https://github.com/salsa-rs/salsa/pull/787))
- Keep edge condvar on stack instead of allocating it in an `Arc` ([#773](https://github.com/salsa-rs/salsa/pull/773))
- allow reuse of cached provisional memos within the same cycle iteration ([#786](https://github.com/salsa-rs/salsa/pull/786))
- Implement `Lookup`/`HashEqLike` for `Arc` ([#784](https://github.com/salsa-rs/salsa/pull/784))
- Normalize imports style ([#779](https://github.com/salsa-rs/salsa/pull/779))
- Clean up `par_map` a bit ([#742](https://github.com/salsa-rs/salsa/pull/742))
- Fix typo in comment ([#777](https://github.com/salsa-rs/salsa/pull/777))
- Document most safety blocks ([#776](https://github.com/salsa-rs/salsa/pull/776))
- Use html directory for mdbook artifact ([#774](https://github.com/salsa-rs/salsa/pull/774))
- Move `verified_final` from `Memo` into `QueryRevisions` ([#769](https://github.com/salsa-rs/salsa/pull/769))
- Use `ThinVec` for `MemoTable`, halving its size ([#770](https://github.com/salsa-rs/salsa/pull/770))
- Remove unnecessary query stack acess in `block_on` ([#771](https://github.com/salsa-rs/salsa/pull/771))
- Replace memo queue with append-only vector ([#767](https://github.com/salsa-rs/salsa/pull/767))
- update boxcar ([#696](https://github.com/salsa-rs/salsa/pull/696))
- Remove extra page indirection in `Table` ([#710](https://github.com/salsa-rs/salsa/pull/710))
- update release steps ([#705](https://github.com/salsa-rs/salsa/pull/705))
- Remove some unnecessary panicking paths in cycle execution ([#765](https://github.com/salsa-rs/salsa/pull/765))
- *(perf)* Pool `ActiveQuerys` in the query stack ([#629](https://github.com/salsa-rs/salsa/pull/629))
- Resolve unwind safety fixme ([#761](https://github.com/salsa-rs/salsa/pull/761))
- Enable Garbage Collection for Interned Values ([#602](https://github.com/salsa-rs/salsa/pull/602))
- bug [salsa-macros]: Improve debug name of tracked methods ([#755](https://github.com/salsa-rs/salsa/pull/755))
- Remove dead code ([#764](https://github.com/salsa-rs/salsa/pull/764))
- Reduce unnecessary conditional work in `deep_verify_memo` ([#759](https://github.com/salsa-rs/salsa/pull/759))
- Use a `Vec` for `CycleHeads` ([#760](https://github.com/salsa-rs/salsa/pull/760))
- Use nextest for miri test runs ([#758](https://github.com/salsa-rs/salsa/pull/758))
- Pin `half` version to prevent CI failure ([#757](https://github.com/salsa-rs/salsa/pull/757))
- rewrite cycle handling to support fixed-point iteration ([#603](https://github.com/salsa-rs/salsa/pull/603))

## [0.19.0](https://github.com/salsa-rs/salsa/compare/salsa-v0.18.0...salsa-v0.19.0) - 2025-03-10

### Fixed

- fix typo
- fix enums bug

### Other

- Have salsa not depend on salsa-macros ([#750](https://github.com/salsa-rs/salsa/pull/750))
- Group versions of packages together for releases ([#751](https://github.com/salsa-rs/salsa/pull/751))
- use `portable-atomic` in `IngredientCache` to compile on `powerpc-unknown-linux-gnu` ([#749](https://github.com/salsa-rs/salsa/pull/749))
- Store view downcaster in function ingredients directly ([#720](https://github.com/salsa-rs/salsa/pull/720))
- Some small perf things ([#744](https://github.com/salsa-rs/salsa/pull/744))
- :replace instead of std::mem::replace ([#746](https://github.com/salsa-rs/salsa/pull/746))
- Cleanup `Cargo.toml`s ([#745](https://github.com/salsa-rs/salsa/pull/745))
- Drop clone requirement for accumulated values
- implement `Update` trait for `IndexMap`, and `IndexSet`
- more correct bounds on `Send` and `Sync` implementation `DeletedEntries`
- replace `arc-swap` with manual `AtomicPtr`
- Remove unnecessary `current_revision` call from `setup_interned_struct`
- Merge pull request #731 from Veykril/veykril/push-nzkwqzxxkxou
- Remove some dynamically dispatched `Database::event` calls
- Lazy fetching
- Add small supertype input benchmark
- Replace a `DashMap` with `RwLock` as writing is rare for it
- address review comments
- Skip memo ingredient index mapping for non enum tracked functions
- Trade off a bit of memory for more speed in `MemoIngredientIndices`
- Introduce Salsa enums
- Cancel duplicate test workflow runs
- implement `Update` trait for `hashbrown::HashMap`
- Move `unwind_if_revision_cancelled` from `ZalsaLocal` to `Zalsa`
- Don't clone strings in benchmarks
- Merge pull request #714 from Veykril/veykril/push-synxntlkqqsq
- Merge pull request #711 from Veykril/veykril/push-stmmwmtprovt
- Merge pull request #715 from Veykril/veykril/push-plwpsqknwulq
- Enforce `unsafe_op_in_unsafe_fn`
- Remove some `ZalsaDatabase::zalsa` calls
- Remove outdated FIXME
- Replace `IngredientCache` lock with atomic primitive
- Reduce method delegation duplication
- Automatically clear the cancellation flag when cancellation completes
- Allow trigger LRU eviction without increasing the current revision
- Simplify `Ingredient::reset_for_new_revision` setup
- Require mut Zalsa access for setting the lru limit
- Split off revision bumping from `zalsa_mut` access
- Update `hashbrown` (0.15) and `hashlink` (0.10)
