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

## Partial deferred

```py
def f(v: int | "Foo"):
    reveal_type(v)  # revealed: int | Foo

class Foo: ...
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
):
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

def f(v: Literal["a", r"b", b"c", "d" "e", "\N{LATIN SMALL LETTER F}", "\x67", """h"""]):
    reveal_type(v)  # revealed: Literal["a", "b", b"c", "de", "f", "g", "h"]
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
c: "Foo"
# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to `Foo`"
d: "Foo" = 1

class Foo: ...

c = Foo()

reveal_type(a)  # revealed: Literal[1]
reveal_type(b)  # revealed: Literal[1]
reveal_type(c)  # revealed: Foo
reveal_type(d)  # revealed: Foo
```

## Parameter

TODO: Add tests once parameter inference is supported

## Invalid expressions

The expressions in these string annotations aren't valid expressions in this context but we
shouldn't panic.

```py
a: "1 or 2"
b: "(x := 1)"
c: "1 + 2"
d: "lambda x: x"
e: "x if True else y"
f: "{'a': 1, 'b': 2}"
g: "{1, 2}"
h: "[i for i in range(5)]"
i: "{i for i in range(5)}"
j: "{i: i for i in range(5)}"
k: "(i for i in range(5))"
l: "await 1"
# error: [invalid-syntax-in-forward-annotation]
m: "yield 1"
# error: [invalid-syntax-in-forward-annotation]
n: "yield from 1"
o: "1 < 2"
p: "call()"
r: "[1, 2]"
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
