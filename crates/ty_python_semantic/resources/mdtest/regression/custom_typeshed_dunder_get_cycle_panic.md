# Custom typeshed `try_call_dunder_get_` cycle panic

Regression for a broader panic repro reduced to a small custom-typeshed setup.

```toml
[environment]
python-version = "3.8"
python-platform = "linux"
typeshed = "/typings"
extra-paths = ["/typings"]
```

```py
from ufoo import bar  # error: [unresolved-import]
```

`/typings/ufoo.pyi`:

```pyi
```

`/typings/stdlib/VERSIONS`:

```text
builtins: 3.0-
types: 3.0-
typing: 3.0-
```

`/typings/stdlib/builtins.pyi`:

```pyi
from typing import (  # noqa: Y022
    TypeVar,
    from types import GenericAlias
_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
class object:
    def __new__(cls, x: str | bytes | bytearray, /, base: SupportsIndex) -> Self: ...
class tuple(Sequence[_T_co]):
```

`/typings/stdlib/types.pyi`:

```pyi
from typing import Any, ClassVar, Literal, Mapping, TypeVar, final, overload  # noqa: Y022
@final
class FunctionType:
    @overload
    def __get__(self, instance: object, owner: type | None = None, /) -> MethodType: ...
class ModuleType:
```

`/typings/stdlib/typing.pyi`:

```pyi
def final(f: _T) -> _T: ...
class TypeVar:
    if sys.version_info >= (3, 12):
        def __init__(
    def NewType(name: str, tp: Any) -> Any: ...
_F = TypeVar("_F", bound=Callable[..., Any])
def overload(func: _F) -> _F: ...
```
