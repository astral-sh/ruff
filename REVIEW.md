# Review decisions

- Fixed (`e394e65de3`) — Preserve final-write ordering for eager comprehension walruses.
- Fixed (`9b99a059f5`) — Leave statically unreachable walrus targets unbound outside the comprehension.
- Fixed (`b63eae4f7d`) — Preserve unused-binding hints for unread comprehension walruses.
- Fixed (`db5d6876a8`) — Preserve prior values and possible unboundness for conditional walruses.
- Fixed (`adeccfb828`) — Conservatively widen directly loop-carried walrus values.
- Fixed (`61c3de88c6`) — Propagate same-comprehension reads to the exported binding.
- Fixed (`5f59e17e3a`) — Resolve exported bindings to their originating walrus for IDE navigation.
- Fixed — Keep statically unreachable function walruses locally owned while leaving their value unbound.
- Rejected — Cross-target loop dependency cycles are outside Issue 162's realistic partial-sum case; modeling them requires a new dependency graph, and Pyright also leaves this case literal-precise.
- Rejected — Nested same-name binders only cause safe over-widening in a contrived case; precise handling requires nested-scope name resolution, while mypy likewise widens to `str`.
- Fixed — Preserve unused-binding usage and target ranges per comprehension walrus definition.
