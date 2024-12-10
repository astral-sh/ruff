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
def f1(v: "int | str"):
    reveal_type(v)  # revealed: int | str

def f2(v: "tuple[int, str]"):
    reveal_type(v)  # revealed: tuple[int, str]
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

class Foo:
    pass
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

class Foo:
    pass
```

## `typing.Literal`

```py
from typing import Literal

def f1(v: Literal["Foo", "Bar"]):
    reveal_type(v)  # revealed: Literal["Foo", "Bar"]

def f2(v: 'Literal["Foo", "Bar"]'):
    reveal_type(v)  # revealed: Literal["Foo", "Bar"]

class Foo:
    pass
```

## Various string kinds

```py
# error: [annotation-raw-string] "Type expressions cannot use raw string literal"
def f1(v: r"int"):
    reveal_type(v)  # revealed: Unknown

# error: [annotation-f-string] "Type expressions cannot use f-strings"
def f2(v: f"int"):
    reveal_type(v)  # revealed: Unknown

# error: [annotation-byte-string] "Type expressions cannot use bytes literal"
def f3(v: b"int"):
    reveal_type(v)  # revealed: Unknown

def f4(v: "int"):
    reveal_type(v)  # revealed: int

# error: [annotation-implicit-concat] "Type expressions cannot span multiple string literals"
def f5(v: "in" "t"):
    reveal_type(v)  # revealed: Unknown

# error: [annotation-escape-character] "Type expressions cannot contain escape characters"
def f6(v: "\N{LATIN SMALL LETTER I}nt"):
    reveal_type(v)  # revealed: Unknown

# error: [annotation-escape-character] "Type expressions cannot contain escape characters"
def f7(v: "\x69nt"):
    reveal_type(v)  # revealed: Unknown

def f8(v: """int"""):
    reveal_type(v)  # revealed: int

# error: [annotation-byte-string] "Type expressions cannot use bytes literal"
def f9(v: "b'int'"):
    reveal_type(v)  # revealed: Unknown
```

## Various string kinds in `typing.Literal`

```py
from typing import Literal

def f(v: Literal["a", r"b", b"c", "d" "e", "\N{LATIN SMALL LETTER F}", "\x67", """h"""]):
    reveal_type(v)  # revealed: Literal["a", "b", "de", "f", "g", "h"] | Literal[b"c"]
```

## Class variables

```py
MyType = int

class Aliases:
    MyType = str

    forward: "MyType"
    not_forward: MyType

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

class Foo:
    pass

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
# error: [forward-annotation-syntax-error]
m: "yield 1"
# error: [forward-annotation-syntax-error]
n: "yield from 1"
o: "1 < 2"
p: "call()"
r: "[1, 2]"
s: "(1, 2)"
```
