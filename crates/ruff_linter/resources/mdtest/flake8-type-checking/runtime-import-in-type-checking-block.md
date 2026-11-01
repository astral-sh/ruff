# `runtime-import-in-type-checking-block` (`TC004`)

With `lint.flake8-type-checking.quote-annotations` enabled, an import inside an
`if TYPE_CHECKING:` block that is used at runtime can be kept in place by quoting its runtime
references instead of moving the import out of the block.

```toml
[lint]
select = ["TC004"]

[lint.flake8-type-checking]
quote-annotations = true
```

## A reference needs an escape sequence

Quoting `Type[Literal["\n"]]` leaves an escape sequence in the forward reference, which tools
like ty can't analyze. A single fix quotes every runtime reference, so the whole fix is
display-only.

```py
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from third_party import Type  # snapshot: runtime-import-in-type-checking-block

def f(x: Type[int]): ...
def g(x: Type[Literal["\n"]]): ...
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
help: Quote references
  |
5 |
  - def f(x: Type[int]): ...
  - def g(x: Type[Literal["\n"]]): ...
6 + def f(x: "Type[int]"): ...
7 + def g(x: "Type[Literal['\\n']]"): ...
  |
note: This is a display-only fix and is likely to be incorrect
```
