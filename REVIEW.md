# Review decisions

- Fixed (`e394e65de3`) — Preserve final-write ordering for eager comprehension walruses.
- Fixed (`9b99a059f5`) — Leave statically unreachable walrus targets unbound outside the comprehension.
- Fixed (`b63eae4f7d`) — Preserve unused-binding hints for unread comprehension walruses.
- Fixed (`db5d6876a8`) — Preserve prior values and possible unboundness for conditional walruses.
- Fixed (`adeccfb828`) — Conservatively widen directly loop-carried walrus values.
- Fixed (`61c3de88c6`) — Propagate same-comprehension reads to the exported binding.
- Fixed (`5f59e17e3a`) — Resolve exported bindings to their originating walrus for IDE navigation.
- Fixed — Keep statically unreachable function walruses locally owned while leaving their value unbound.
