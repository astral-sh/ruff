# `bad-exit-annotation` (`PYI036`)

```toml
[lint]
select = ["PYI036"]
```

## Stringized annotations

A quoted annotation means the same thing to a type checker as the unquoted one, so quoted forms of
valid annotations are accepted:

```py
import types
from types import TracebackType

class Quoted:
    def __exit__(self, typ: "type[BaseException] | None", exc: "BaseException | None", tb: "TracebackType | None") -> None: ...  # no diagnostic
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: "TracebackType | None") -> None: ...  # no diagnostic

class QuotedQualified:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: "types.TracebackType | None") -> None: ...  # no diagnostic

class QuotedObject:
    def __exit__(self, typ: "object", exc: "object", tb: "object") -> None: ...  # no diagnostic
```

Quoted annotations that would be invalid unquoted are still reported:

```py
from types import TracebackType

class Bad:
    def __exit__(self, typ: "type[Exception] | None", exc: BaseException | None, tb: TracebackType | None) -> None: ...  # error: [bad-exit-annotation]
    async def __aexit__(self, typ: type[BaseException] | None, exc: "Exception | None", tb: TracebackType | None) -> None: ...  # error: [bad-exit-annotation]

class BadTraceback:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: "Exception | None") -> None: ...  # error: [bad-exit-annotation]
```

## Quoted references to imports in a type-checking block

`TC004` with `quote-annotations = true` quotes the annotation when the import is only available
for type checking, which should not trigger this rule:

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import TracebackType

class Demo:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: "TracebackType | None") -> None: ...  # no diagnostic
```

## Star-args

```py
class Good:
    def __exit__(self, *args: "object") -> None: ...  # no diagnostic
    async def __aexit__(self, *args: "object") -> None: ...  # no diagnostic

class Bad:
    def __exit__(self, *args: "int") -> None: ...  # snapshot: bad-exit-annotation
```

```snapshot
error[PYI036]: Star-args in `__exit__` should be annotated with `object`
 --> src/mdtest_snippet.py:6:31
  |
6 |     def __exit__(self, *args: "int") -> None: ...  # snapshot: bad-exit-annotation
  |                               ^^^^^
help: Annotate star-args with `object`
  |
5 | class Bad:
  -     def __exit__(self, *args: "int") -> None: ...  # snapshot: bad-exit-annotation
6 +     def __exit__(self, *args: object) -> None: ...  # snapshot: bad-exit-annotation
  |
```

## Overloads

```py
from types import TracebackType
from typing import overload

class Good:
    @overload
    def __exit__(self, typ: "None", exc: "None", tb: "None") -> None: ...  # no diagnostic
    @overload
    def __exit__(self, typ: "type[BaseException]", exc: "BaseException", tb: "TracebackType") -> None: ...  # no diagnostic
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...

class Bad:
    @overload
    def __exit__(self, typ: "None", exc: "None", tb: "None") -> None: ...  # no diagnostic
    @overload
    def __exit__(self, typ: "type[BaseException]", exc: "Exception", tb: "TracebackType") -> None: ...  # error: [bad-exit-annotation]
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...
```
