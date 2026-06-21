# `runtime-import-in-type-checking-block` (`TC004`)

With \[`lint.flake8-type-checking.quote-annotations`\] enabled, an import inside an
`if TYPE_CHECKING:` block that is used at runtime can be kept in place by quoting its runtime
references instead of moving the import out of the block.

Quoting is all-or-nothing across the import's references. If any runtime reference cannot be
quoted without leaving an escape, no fix is offered. Quoting only some references would leave
the others referring to a name that the type-checking import does not provide at runtime.

```toml
target-version = "py313"

[lint]
select = ["TC004"]

[lint.flake8-type-checking]
quote-annotations = true
```

## Every reference can be quoted

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from third_party import Type  # snapshot: runtime-import-in-type-checking-block

def f(x: Type[int]): ...
def g(x: Type[str]): ...
```

```snapshot
error[TC004]: Quote references to `third_party.Type`. Import is in a type-checking block.
 --> src/mdtest_snippet.py:4:29
  |
4 |     from third_party import Type  # snapshot: runtime-import-in-type-checking-block
  |                             ^^^^
5 |
6 | def f(x: Type[int]): ...
  |          ---- Used at runtime here
  |
help: Quote references
3 | if TYPE_CHECKING:
4 |     from third_party import Type  # snapshot: runtime-import-in-type-checking-block
5 |
  - def f(x: Type[int]): ...
  - def g(x: Type[str]): ...
6 + def f(x: "Type[int]"): ...
7 + def g(x: "Type[str]"): ...
note: This is an unsafe fix and may change runtime behavior
```

## A reference cannot be quoted

`Type[Literal["'", '"', "'''", '"""']]` uses every quote style, so it has no escape-free
wrapper. The diagnostic is still reported, but the snapshot carries no fix edit, so no
references are quoted.

```py
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from third_party import Type  # snapshot: runtime-import-in-type-checking-block

def f(x: Type[int]): ...
def g(x: Type[Literal["'", '"', "'''", '"""']]): ...
```

```snapshot
error[TC004]: Quote references to `third_party.Type`. Import is in a type-checking block.
 --> src/mdtest_snippet.py:4:29
  |
4 |     from third_party import Type  # snapshot: runtime-import-in-type-checking-block
  |                             ^^^^
5 |
6 | def f(x: Type[int]): ...
  |          ---- Used at runtime here
  |
help: Quote references
```
