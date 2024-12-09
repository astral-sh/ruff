# String annotations

## Simple

```py
def f() -> "int":
    return 1

reveal_type(f())  # revealed: int
```

## Nested

```py
def f() -> "'int'":
    return 1

reveal_type(f())  # revealed: int
```

## Type expression

```py
def f1() -> "int | str":
    return 1

def f2() -> "tuple[int, str]":
    return 1

reveal_type(f1())  # revealed: int | str
reveal_type(f2())  # revealed: tuple[int, str]
```

## Partial

```py
def f() -> tuple[int, "str"]:
    return 1

reveal_type(f())  # revealed: tuple[int, str]
```

## Deferred

```py
def f() -> "Foo":
    return Foo()

class Foo:
    pass

reveal_type(f())  # revealed: Foo
```

## Deferred (undefined)

```py
# error: [unresolved-reference]
def f() -> "Foo":
    pass

reveal_type(f())  # revealed: Unknown
```

## Partial deferred

```py
def f() -> int | "Foo":
    return 1

class Foo:
    pass

reveal_type(f())  # revealed: int | Foo
```

## `typing.Literal`

```py
from typing import Literal

def f1() -> Literal["Foo", "Bar"]:
    return "Foo"

def f2() -> 'Literal["Foo", "Bar"]':
    return "Foo"

class Foo:
    pass

reveal_type(f1())  # revealed: Literal["Foo", "Bar"]
reveal_type(f2())  # revealed: Literal["Foo", "Bar"]
```

## Various string kinds

```py
# error: [raw-string-type-annotation] "Type expressions cannot use raw string literal"
def f1() -> r"int":
    return 1

# error: [fstring-type-annotation] "Type expressions cannot use f-strings"
def f2() -> f"int":
    return 1

# error: [byte-string-type-annotation] "Type expressions cannot use bytes literal"
def f3() -> b"int":
    return 1

def f4() -> "int":
    return 1

# error: [implicit-concatenated-string-type-annotation] "Type expressions cannot span multiple string literals"
def f5() -> "in" "t":
    return 1

# error: [escape-character-in-forward-annotation] "Type expressions cannot contain escape characters"
def f6() -> "\N{LATIN SMALL LETTER I}nt":
    return 1

# error: [escape-character-in-forward-annotation] "Type expressions cannot contain escape characters"
def f7() -> "\x69nt":
    return 1

def f8() -> """int""":
    return 1

# error: [byte-string-type-annotation] "Type expressions cannot use bytes literal"
def f9() -> "b'int'":
    return 1

reveal_type(f1())  # revealed: Unknown
reveal_type(f2())  # revealed: Unknown
reveal_type(f3())  # revealed: Unknown
reveal_type(f4())  # revealed: int
reveal_type(f5())  # revealed: Unknown
reveal_type(f6())  # revealed: Unknown
reveal_type(f7())  # revealed: Unknown
reveal_type(f8())  # revealed: int
reveal_type(f9())  # revealed: Unknown
```

## Various string kinds in `typing.Literal`

```py
from typing import Literal

def f() -> Literal["a", r"b", b"c", "d" "e", "\N{LATIN SMALL LETTER F}", "\x67", """h"""]:
    return "normal"

reveal_type(f())  # revealed: Literal["a", "b", "de", "f", "g", "h"] | Literal[b"c"]
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
# error: [invalid-syntax-in-forward-annotation]
m: "yield 1"
# error: [invalid-syntax-in-forward-annotation]
n: "yield from 1"
o: "1 < 2"
p: "call()"
r: "[1, 2]"
s: "(1, 2)"
```
