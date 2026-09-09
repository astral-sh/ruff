# `lazy-import-immediately-resolved` (`TID255`)

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]
flake8-tidy-imports.require-lazy = "all"
```

## Required lazy imports

`TID255` ignores imports that are required to be lazy to avoid a conflict with `TID254`, even if the
import is resolved immediately.

```py
import foo  # snapshot: lazy-import-mismatch

class Bar(foo.Base): ...
```

```snapshot
error[TID254]: Use a `lazy` import instead of an eager import
 --> src/mdtest_snippet.py:1:8
  |
1 | import foo  # snapshot: lazy-import-mismatch
  |        ^^^
help: Convert to a lazy import
  |
  - import foo  # snapshot: lazy-import-mismatch
1 + lazy import foo  # snapshot: lazy-import-mismatch
2 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Partially required lazy imports

`TID255` still reports immediately resolved names outside `require-lazy`, even when another name in
the same import is required to be lazy.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]
flake8-tidy-imports.require-lazy = ["foo", "pkg.Base"]
```

```py
lazy import foo as required, bar
lazy from pkg import Base as RequiredBase, OtherBase

required.value
RequiredBase()
bar.value  # error: [lazy-import-immediately-resolved]
OtherBase()  # error: [lazy-import-immediately-resolved]
```
