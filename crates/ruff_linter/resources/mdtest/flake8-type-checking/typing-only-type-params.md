# Treat PEP 695/696 type parameter bounds/constraints/defaults as typing-only

```toml
preview = true
target-version = "py313"
lint.select = [
    "typing-only-first-party-import",
    "typing-only-third-party-import",
    "typing-only-standard-library-import"
]
```

The expressions for PEP 695 type parameter bounds and constraints are never
evaluated at runtime, unless explicitly accessed through runtime introspection.

It is very rare that runtime type checkers care about the bounds/constraints of a type parameter.

## Type parameter bounds are typing only

```py
from .anndata import AnnData  # snapshot: typing-only-first-party-import

def foo[T: AnnData](a: T) -> T: ...
```

```snapshot
error[TC001]: Move application import `.anndata.AnnData` into a type-checking block
 --> src/mdtest_snippet.py:1:22
  |
1 | from .anndata import AnnData  # snapshot: typing-only-first-party-import
  |                      ^^^^^^^
help: Move into type-checking block
  |
  - from .anndata import AnnData  # snapshot: typing-only-first-party-import
1 + from typing import TYPE_CHECKING
2 +
3 + if TYPE_CHECKING:
4 +     from .anndata import AnnData
5 |
  |
note: This is an unsafe fix and may change runtime behavior
```

```py
from pandas import DataFrame  # snapshot: typing-only-third-party-import

class Bar[T: DataFrame]: ...
```

```snapshot
error[TC002]: Move third-party import `pandas.DataFrame` into a type-checking block
 --> src/mdtest_snippet.py:4:20
  |
4 | from pandas import DataFrame  # snapshot: typing-only-third-party-import
  |                    ^^^^^^^^^
help: Move into type-checking block
  |
3 | def foo[T: AnnData](a: T) -> T: ...
  - from pandas import DataFrame  # snapshot: typing-only-third-party-import
4 + from typing import TYPE_CHECKING
5 +
6 + if TYPE_CHECKING:
7 +     from pandas import DataFrame
8 |
  |
note: This is an unsafe fix and may change runtime behavior
```

```py
import io  # snapshot: typing-only-standard-library-import

type Baz[T: io.BytesIO] = ...
```

```snapshot
error[TC003]: Move standard library import `io` into a type-checking block
 --> src/mdtest_snippet.py:7:8
  |
7 | import io  # snapshot: typing-only-standard-library-import
  |        ^^
help: Move into type-checking block
   |
6  | class Bar[T: DataFrame]: ...
   - import io  # snapshot: typing-only-standard-library-import
7  + from typing import TYPE_CHECKING
8  +
9  + if TYPE_CHECKING:
10 +     import io
11 |
   |
note: This is an unsafe fix and may change runtime behavior
```

## Type parameter constraints are typing only

```py
import io  # error: typing-only-standard-library-import
from pandas import DataFrame  # error: typing-only-third-party-import
from .anndata import AnnData  # error: typing-only-first-party-import


def f[T: (io.BytesIO, DataFrame, AnnData)](a: T) -> T: ...
```

## Type parameter defaults are typing only

```py
import io  # error: typing-only-standard-library-import
from pandas import DataFrame  # error: typing-only-third-party-import
from .anndata import AnnData  # error: typing-only-first-party-import


def foo[T: object = AnnData](a: T) -> T: ...
class Bar[*Ts = DataFrame]: ...
type Baz[**P = io.BytesIO] = ...
```
