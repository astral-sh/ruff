# String annotations

## Simple

```py
def f(v: "int"):
    reveal_type(v)  # revealed: int
```

## Nested

```py
def f(v: "'int'"):
    reveal_type(v)  # revealed: int
```

## Type expression

```py
def f1(v: "int | str", w: "tuple[int, str]"):
    reveal_type(v)  # revealed: int | str
    reveal_type(w)  # revealed: tuple[int, str]
```

## Partial

```py
def f(v: tuple[int, "str"]):
    reveal_type(v)  # revealed: tuple[int, str]
```

## Deferred

```py
def f(v: "Foo"):
    reveal_type(v)  # revealed: Foo

class Foo: ...
```

## Deferred (undefined)

```py
# error: [unresolved-reference]
def f(v: "Foo"):
    reveal_type(v)  # revealed: Unknown
```

## Partially deferred annotations

### Python less than 3.14

"Partially stringified" PEP-604 unions can raise `TypeError` on Python \<3.14; we try to detect this
common runtime error:

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.13"
```

```py
from typing import TypeVar, Callable, Protocol, TypedDict

class TD(TypedDict): ...

class P(Protocol):
    x: int

T = TypeVar("T")

# fmt: off
def f(
    # error: [unsupported-operator] "String annotations are not supported in PEP-604 unions on Python <3.14"
    a: int | "Foo",
    # error: [unsupported-operator]
    b: int | "memoryview" | bytes,
    # error: [unsupported-operator]
    c: "TD" | None,
    # error: [unsupported-operator]
    d: "P" | None,
    # fine: `TypeVar.__or__` accepts strings at runtime
    e: T | "Foo",
    # fine: _SpecialForm.__ror__` accepts strings at runtime
    f: "Foo" | Callable[..., None],
):
    reveal_type(a)  # revealed: int | Foo
    reveal_type(b)  # revealed: int | memoryview[int] | bytes
    reveal_type(c)  # revealed: TD | None
    reveal_type(d)  # revealed: P | None
    reveal_type(e)  # revealed: T@f | Foo
    reveal_type(f)  # revealed: Foo | ((...) -> None)

# fmt: on

class Foo: ...

# error: [unsupported-operator]
X = list["int" | None]
```

### Python less than 3.14 in a stub file

This error is never emitted on stub files, because they are never executed at runtime:

```toml
[environment]
python-version = "3.13"
```

```pyi
# fine
def f(x: "int" | None): ...
```

### Python less than 3.14 with `__future__` annotations

The errors can be avoided in some situations by using `__future__` annotations on Pythonn \<3.14:

```toml
[environment]
python-version = "3.13"
```

```py
from __future__ import annotations

def f(v: int | "Foo"):  # fine
    reveal_type(v)  # revealed: int | Foo

class Foo: ...

# TODO: ideally we would emit `unsupported-operator` here;
# it still fails at runtime despite `__future__.annotations`
X = list["int" | None]
```

### Python >=3.14

Runtime errors are also less common for partially stringified annotations if the Python version
being used is >=3.14:

```toml
[environment]
python-version = "3.14"
```

```py
def f(v: int | "Foo"):  # fine
    reveal_type(v)  # revealed: int | Foo

class Foo: ...

# TODO: ideally we would emit `unsupported-operator` here;
# it still fails at runtime even on Python 3.14+
X = list["int" | None]
```

## `typing.Literal`

```py
from typing import Literal

def f1(v: Literal["Foo", "Bar"], w: 'Literal["Foo", "Bar"]'):
    reveal_type(v)  # revealed: Literal["Foo", "Bar"]
    reveal_type(w)  # revealed: Literal["Foo", "Bar"]

class Foo: ...
```

## Various string kinds

```py
def f1(
    # error: [raw-string-type-annotation] "Type expressions cannot use raw string literal"
    a: r"int",
    # error: [fstring-type-annotation] "Type expressions cannot use f-strings"
    b: f"int",
    # error: [byte-string-type-annotation] "Type expressions cannot use bytes literal"
    c: b"int",
    d: "int",
    # error: [implicit-concatenated-string-type-annotation] "Type expressions cannot span multiple string literals"
    e: "in" "t",
    # error: [escape-character-in-forward-annotation] "Type expressions cannot contain escape characters"
    f: "\N{LATIN SMALL LETTER I}nt",
    # error: [escape-character-in-forward-annotation] "Type expressions cannot contain escape characters"
    g: "\x69nt",
    h: """int""",
    # error: [byte-string-type-annotation] "Type expressions cannot use bytes literal"
    i: "b'int'",
):  # fmt:skip
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: Unknown
    reveal_type(c)  # revealed: Unknown
    reveal_type(d)  # revealed: int
    reveal_type(e)  # revealed: Unknown
    reveal_type(f)  # revealed: Unknown
    reveal_type(g)  # revealed: Unknown
    reveal_type(h)  # revealed: int
    reveal_type(i)  # revealed: Unknown
```

## Various string kinds in `typing.Literal`

```py
from typing import Literal

def f(v: Literal["a", r"b", b"c", "d" "e", "\N{LATIN SMALL LETTER F}", "\x67", """h"""]):  # fmt:skip
    reveal_type(v)  # revealed: Literal["a", "b", "de", "f", "g", "h", b"c"]
```

## Class variables

```py
MyType = int

class Aliases:
    MyType = str

    forward: "MyType" = "value"
    not_forward: MyType = "value"

reveal_type(Aliases.forward)  # revealed: str
reveal_type(Aliases.not_forward)  # revealed: str
```

## Annotated assignment

```py
a: "int" = 1
b: "'int'" = 1
# error: [invalid-syntax-in-forward-annotation] "too many levels of nested string annotations; remove the redundant nested quotes"
c: """'"int"'""" = 1
d: "Foo"
# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to `Foo`"
e: "Foo" = 1
# error: [invalid-syntax-in-forward-annotation] "nested string annotation is too long; remove the redundant nested quotes"
f: "'str | int | bool | Foo | Bar'" = 1

class Foo: ...

d = Foo()

reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[1]
reveal_type(c)  # revealed: Literal[1]
reveal_type(d)  # revealed: Foo
reveal_type(e)  # revealed: Foo
reveal_type(f)  # revealed: Literal[1]
```

## Parameter

TODO: Add tests once parameter inference is supported

## Invalid expressions

The expressions in these string annotations aren't valid expressions in this context but we
shouldn't panic.

```py
# Regression test for https://github.com/astral-sh/ty/issues/1865
# error: [fstring-type-annotation]
stringified_fstring_with_conditional: "f'{1 if 1 else 1}'"
# error: [fstring-type-annotation]
stringified_fstring_with_boolean_expression: "f'{1 or 2}'"
# error: [fstring-type-annotation]
stringified_fstring_with_generator_expression: "f'{(i for i in range(5))}'"
# error: [fstring-type-annotation]
stringified_fstring_with_list_comprehension: "f'{[i for i in range(5)]}'"
# error: [fstring-type-annotation]
stringified_fstring_with_dict_comprehension: "f'{ {i: i for i in range(5)} }'"
# error: [fstring-type-annotation]
stringified_fstring_with_set_comprehension: "f'{ {i for i in range(5)} }'"

# error: [invalid-type-form]
a: "1 or 2"
# error: [invalid-type-form]
b: "(x := 1)"
# error: [invalid-type-form]
c: "1 + 2"
# Regression test for https://github.com/astral-sh/ty/issues/1847
# error: [invalid-type-form]
c2: "a*(i for i in [])"
# error: [invalid-type-form]
d: "lambda x: x"
# error: [invalid-type-form]
e: "x if True else y"
# error: [invalid-type-form]
f: "{'a': 1, 'b': 2}"
# error: [invalid-type-form]
g: "{1, 2}"
# error: [invalid-type-form]
h: "[i for i in range(5)]"
# error: [invalid-type-form]
i: "{i for i in range(5)}"
# error: [invalid-type-form]
j: "{i: i for i in range(5)}"
# error: [invalid-type-form]
k: "(i for i in range(5))"
# error: [invalid-type-form]
l: "await 1"
# error: [invalid-syntax-in-forward-annotation]
m: "yield 1"
# error: [invalid-syntax-in-forward-annotation]
n: "yield from 1"
# error: [invalid-type-form]
o: "1 < 2"
# error: [invalid-type-form]
p: "call()"
# error: [invalid-type-form] "List literals are not allowed"
# error: [invalid-type-form] "Int literals are not allowed"
# error: [invalid-type-form] "Int literals are not allowed"
r: "[1, 2]"
# error: [invalid-type-form] "Tuple literals are not allowed"
# error: [invalid-type-form] "Int literals are not allowed"
# error: [invalid-type-form] "Int literals are not allowed"
s: "(1, 2)"
```

## Multi line annotation

Quoted type annotations should be parsed as if surrounded by parentheses.

```py
def valid(
    a1: """(
      int |
      str
  )
  """,
    a2: """
     int |
       str
  """,
):
    reveal_type(a1)  # revealed: int | str
    reveal_type(a2)  # revealed: int | str

def invalid(
    # error: [invalid-syntax-in-forward-annotation]
    a1: """
  int |
str)
""",
    # error: [invalid-syntax-in-forward-annotation]
    a2: """
  int) |
str
""",
    # error: [invalid-syntax-in-forward-annotation]
    a3: """
      (int)) """,
):
    pass
```
