# `typing-only-third-party-import` (`TC002`)

With \[`lint.flake8-type-checking.quote-annotations`\] enabled, a third-party import that is
used only in annotations is moved into an `if TYPE_CHECKING:` block and its runtime
references are quoted.

Quoting is all-or-nothing across the import's references. If any runtime reference cannot be
quoted without leaving an escape, the import is left in place. Quoting only some references
while moving the import would leave the others pointing at a name that is no longer available
at runtime.

```toml
target-version = "py313"

[lint]
select = ["TC002"]

[lint.flake8-type-checking]
quote-annotations = true
```

## Every reference can be quoted

The import moves into the type-checking block and both references are quoted.

```py
from third_party import Type  # snapshot: typing-only-third-party-import

def f(x: Type[int]): ...
def g(x: Type[str]): ...
```

```snapshot
error[TC002]: Move third-party import `third_party.Type` into a type-checking block
 --> src/mdtest_snippet.py:1:25
  |
1 | from third_party import Type  # snapshot: typing-only-third-party-import
  |                         ^^^^
  |
help: Move into type-checking block
  - from third_party import Type  # snapshot: typing-only-third-party-import
1 + from typing import TYPE_CHECKING
2 |
  - def f(x: Type[int]): ...
  - def g(x: Type[str]): ...
3 + if TYPE_CHECKING:
4 +     from third_party import Type
5 +
6 + def f(x: "Type[int]"): ...
7 + def g(x: "Type[str]"): ...
note: This is an unsafe fix and may change runtime behavior
```

## A reference cannot be quoted

`Type[Literal["'", '"', "'''", '"""']]` uses every quote style, so it has no escape-free
wrapper. The diagnostic is still reported, but the snapshot carries no fix edit, so the
import is left in place.

```py
from typing import Literal
from third_party import Type  # snapshot: typing-only-third-party-import

def f(x: Type[int]): ...
def g(x: Type[Literal["'", '"', "'''", '"""']]): ...
```

```snapshot
error[TC002]: Move third-party import `third_party.Type` into a type-checking block
 --> src/mdtest_snippet.py:2:25
  |
2 | from third_party import Type  # snapshot: typing-only-third-party-import
  |                         ^^^^
  |
help: Move into type-checking block
```
