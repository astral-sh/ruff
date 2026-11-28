# `literal-union-with-supertype` (`PYI051`)

```toml
[lint]
select = ["PYI051"]
```

## Redundant literal members

```py
import typing
from typing import Literal, TypeAlias, Union

A: str | Literal["foo"]  # snapshot: literal-union-with-supertype
```

```snapshot
error[PYI051]: `Literal["foo"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:4:18
  |
4 | A: str | Literal["foo"]  # snapshot: literal-union-with-supertype
  |                  ^^^^^
```

```py
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
B: TypeAlias = typing.Union[Literal[b"bar", b"foo"], bytes, str]
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
C: TypeAlias = typing.Union[Literal[5], int, typing.Union[Literal["foo"], str]]
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
D: TypeAlias = typing.Union[Literal[b"str_bytes", 42], bytes, int]
E: TypeAlias = typing.Union[typing.Union[typing.Union[typing.Union[Literal["foo"], str]]]]  # error: [literal-union-with-supertype]
F: TypeAlias = typing.Union[str, typing.Union[typing.Union[typing.Union[Literal["foo"], int]]]]  # error: [literal-union-with-supertype]
G: typing.Union[str, typing.Union[typing.Union[typing.Union[Literal["foo"], int]]]]  # error: [literal-union-with-supertype]

# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
def func(x: complex | Literal[1J], y: Union[Literal[3.14], float]): ...
```

## Non-redundant literal members

```py
import typing
from typing import Literal, TypeAlias

A: Literal["foo"]
B: TypeAlias = Literal[b"bar", b"foo"]
C: TypeAlias = typing.Union[Literal[5], Literal["foo"]]
D: TypeAlias = Literal[b"str_bytes", 42]

def func(x: Literal[1J], y: Literal[3.14]): ...
```

## Stub files

### Redundant literal members

```pyi
import typing
from typing import Literal, TypeAlias, Union

A: str | Literal["foo"]  # error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
B: TypeAlias = typing.Union[Literal[b"bar", b"foo"], bytes, str]
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
C: TypeAlias = typing.Union[Literal[5], int, typing.Union[Literal["foo"], str]]
# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
D: TypeAlias = typing.Union[Literal[b"str_bytes", 42], bytes, int]
E: TypeAlias = typing.Union[typing.Union[typing.Union[typing.Union[Literal["foo"], str]]]]  # error: [literal-union-with-supertype]
F: TypeAlias = typing.Union[str, typing.Union[typing.Union[typing.Union[Literal["foo"], int]]]]  # error: [literal-union-with-supertype]
G: typing.Union[str, typing.Union[typing.Union[typing.Union[Literal["foo"], int]]]]  # error: [literal-union-with-supertype]

# error: [literal-union-with-supertype]
# error: [literal-union-with-supertype]
def func(x: complex | Literal[1J], y: Union[Literal[3.14], float]): ...
```

### Non-redundant literal members

```pyi
import typing
from typing import Literal, TypeAlias

A: Literal["foo"]
B: TypeAlias = Literal[b"bar", b"foo"]
C: TypeAlias = typing.Union[Literal[5], Literal["foo"]]
D: TypeAlias = Literal[b"str_bytes", 42]

def func(x: Literal[1J], y: Literal[3.14]): ...
```
