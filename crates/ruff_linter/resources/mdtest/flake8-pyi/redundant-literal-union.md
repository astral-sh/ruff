# `redundant-literal-union` (`PYI051`)

```toml
target-version = "py310"

[lint]
preview = true
select = ["PYI051"]
```

## Literal members with a builtin supertype in the union

A `Literal` member is redundant when the union also contains the builtin type that the member is an instance of. `str` already admits every string, so `Literal["A"]` contributes nothing to `Literal["A", b"B"] | str`. The fix deletes only the redundant member and leaves the rest of the `Literal` alone.

```py
from typing import Literal

x: Literal["A", b"B"] | str  # snapshot: redundant-literal-union
```

```snapshot
error[PYI051]: `Literal["A"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:3:12
  |
3 | x: Literal["A", b"B"] | str  # snapshot: redundant-literal-union
  |            ^^^
help: Remove redundant literal member
  |
2 |
  - x: Literal["A", b"B"] | str  # snapshot: redundant-literal-union
3 + x: Literal[b"B"] | str  # snapshot: redundant-literal-union
4 | a: Literal[1, "A"] | int  # error: [redundant-literal-union]
  |
```

Each of the six builtin types a `Literal` member can have is recognized.

```py
a: Literal[1, "A"] | int  # error: [redundant-literal-union]
b: Literal["A", 1] | str  # error: [redundant-literal-union]
c: Literal[True, 1] | bool  # error: [redundant-literal-union]
d: Literal[3.14, 1] | float  # error: [redundant-literal-union]
e: Literal[b"A", 1] | bytes  # error: [redundant-literal-union]
f: Literal[1j, 1] | complex  # error: [redundant-literal-union]
```

## Literal members without a matching builtin

A member is only redundant with the builtin type it is an instance of. `bool` is a subclass of `int` at runtime, but `Literal[1]` is not a `bool`, and the typing spec's promotions from `int` to `float` to `complex` do not make `Literal[1]` redundant with `float` either.

```py
from typing import Literal

a: Literal[1] | bool
b: Literal[1] | float
c: Literal[1] | complex
d: Literal[3.14] | complex
e: Literal["A"] | bytes
```

`None` and `...` are not instances of any of those builtins.

```py
f: Literal[None, ...] | str
```

A `Literal` outside a union has nothing to be redundant with.

```py
from typing import TypeAlias

g: Literal["A"]
H: TypeAlias = Literal[b"A", 42]
```

## Unions where every literal member is redundant

`Literal[]` is a syntax error, so when the union makes every member of a `Literal` redundant, the whole `Literal` is deleted along with the `|` that joined it to the rest of the union.

```py
from typing import Literal

x: str | Literal["A"]  # snapshot: redundant-literal-union
```

```snapshot
error[PYI051]: `Literal["A"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:3:18
  |
3 | x: str | Literal["A"]  # snapshot: redundant-literal-union
  |                  ^^^
help: Remove redundant literal member
  |
2 |
  - x: str | Literal["A"]  # snapshot: redundant-literal-union
3 + x: str  # snapshot: redundant-literal-union
4 | # error: [redundant-literal-union]
  |
```

Each redundant member is still reported separately, and the diagnostics share that one deletion.

```py
# error: [redundant-literal-union]
# error: [redundant-literal-union]
y: str | Literal["A", "B"]
```

## `typing.Union`

Inside `typing.Union[...]` the member is deleted along with one of the commas around it. The fix never rewrites the union itself, so a `typing.Union[...]` stays a `typing.Union[...]` rather than becoming `X | Y`. Deleting all but one member therefore leaves a single-element `typing.Union[X]` behind, which `non-pep604-annotation-union` can go on to simplify.

```py
import typing
from typing import Literal

x: typing.Union[Literal["A"], str]  # snapshot: redundant-literal-union
```

```snapshot
error[PYI051]: `Literal["A"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:4:25
  |
4 | x: typing.Union[Literal["A"], str]  # snapshot: redundant-literal-union
  |                         ^^^
help: Remove redundant literal member
  |
3 |
  - x: typing.Union[Literal["A"], str]  # snapshot: redundant-literal-union
4 + x: typing.Union[str]  # snapshot: redundant-literal-union
5 | from typing import TypeAlias, Union
  |
```

`Union` imported directly behaves identically.

```py
from typing import TypeAlias, Union

# error: [redundant-literal-union]
# error: [redundant-literal-union]
A: TypeAlias = Union[Literal[b"bar", b"foo"], bytes, str]

# error: [redundant-literal-union]
# error: [redundant-literal-union]
B: TypeAlias = Union[Literal[b"str_bytes", 42], bytes, int]
```

## Nested unions

A member is redundant with a builtin found anywhere in the union, however deeply the union is nested, and it is deleted from the union that directly contains it.

```py
import typing
from typing import Literal, TypeAlias

# error: [redundant-literal-union]
# error: [redundant-literal-union]
A: TypeAlias = typing.Union[Literal[5], int, typing.Union[Literal["foo"], str]]

B: TypeAlias = typing.Union[str, typing.Union[typing.Union[Literal["foo"], int]]]  # error: [redundant-literal-union]
C: str | (Literal["foo"] | int)  # error: [redundant-literal-union]
```

## Several `Literal` members in one union

Every redundant member across every `Literal` in the union gets its own diagnostic.

```py
from typing import Literal

# error: [redundant-literal-union]
# error: [redundant-literal-union]
# error: [redundant-literal-union]
# error: [redundant-literal-union]
x: Literal["A", "B", b"c"] | Literal["D", b"f"] | Literal["G"] | str
```

## Comments

A comment inside the deleted range is deleted with it, so the fix is marked unsafe whenever the deletion reaches a comment.

```py
from typing import Literal

x: (
    str
    # this member says nothing that `str` does not
    | Literal["A"]  # snapshot: redundant-literal-union
)
```

```snapshot
error[PYI051]: `Literal["A"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:6:15
  |
6 |     | Literal["A"]  # snapshot: redundant-literal-union
  |               ^^^
help: Remove redundant literal member
  |
3 | x: (
  -     str
  -     # this member says nothing that `str` does not
  -     | Literal["A"]  # snapshot: redundant-literal-union
4 +     str  # snapshot: redundant-literal-union
5 | )
  |
note: This is an unsafe fix and may change runtime behavior
```

A comment outside the deleted range keeps its meaning, so the fix stays safe.

```py
y: Literal[  # one of
    # error: [redundant-literal-union]
    "A",
    b"B",
] | str
```

## Parenthesized union members

Parentheses around either operand of a `|` are preserved rather than half-deleted.

```py
from typing import Literal

a: (str) | (Literal["A"])  # error: [redundant-literal-union]
b: (Literal["A"]) | (str)  # error: [redundant-literal-union]
```

## Unions the fix leaves alone

No fix is offered where deleting the `Literal` would leave an empty `typing.Union[]`, which is a syntax error, or where the `Literal` is parenthesized inside a `typing.Union[...]`, where deleting the subscript's own range would strip the closing parenthesis but not the opening one. The diagnostic is still reported.

```py
from typing import Literal, Union

a: Union[Literal["A"]] | str  # error: [redundant-literal-union]
b: Union[Literal["A"],] | str  # error: [redundant-literal-union]
c: Union[(Literal["A"]), str]  # error: [redundant-literal-union]
```

## Stringized annotations

A quoted annotation is fixed in place, inside the quotes.

```py
from typing import Literal

a: "Literal['A'] | str"  # error: [redundant-literal-union]
```

An implicitly concatenated annotation is reparsed from a buffer of its own, so its expression ranges do not point back into the source and cannot be turned into edits. The redundancy is reported without a fix. Whether a name inside such an annotation resolves at all depends on where the buffer's offsets happen to fall relative to the import that binds it, which is why the first union member here is long.

```py
b: "LongAliasNameForTesting | str | Literal['A']" ""  # error: [redundant-literal-union]
```

## Stub files

Stub files are checked the same way.

```pyi
import typing
from typing import Literal, TypeAlias, Union

A: str | Literal["foo"]  # error: [redundant-literal-union]

# error: [redundant-literal-union]
# error: [redundant-literal-union]
B: TypeAlias = typing.Union[Literal[b"bar", b"foo"], bytes, str]

# error: [redundant-literal-union]
# error: [redundant-literal-union]
def func(x: complex | Literal[1j], y: Union[Literal[3.14], float]) -> None: ...
```

## Without preview

Outside preview the rule reports the same diagnostics but offers no fix.

```toml
[lint]
preview = false
select = ["PYI051"]
```

```py
from typing import Literal

x: Literal["A", b"B"] | str  # snapshot: redundant-literal-union
```

```snapshot
error[PYI051]: `Literal["A"]` is redundant in a union with `str`
 --> src/mdtest_snippet.py:3:12
  |
3 | x: Literal["A", b"B"] | str  # snapshot: redundant-literal-union
  |            ^^^
help: Remove redundant literal member
```
