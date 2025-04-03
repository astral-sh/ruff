# `LiteralString`

`LiteralString` represents a string that is either defined directly within the source code or is
made up of such components.

Parts of the testcases defined here were adapted from [the specification's examples][1].

## Usages

### Valid places

It can be used anywhere a type is accepted:

```py
from typing_extensions import LiteralString

x: LiteralString

def f():
    reveal_type(x)  # revealed: LiteralString
```

### Within `Literal`

`LiteralString` cannot be used within `Literal`:

```py
from typing_extensions import Literal, LiteralString

bad_union: Literal["hello", LiteralString]  # error: [invalid-type-form]
bad_nesting: Literal[LiteralString]  # error: [invalid-type-form]
```

### Parameterized

`LiteralString` cannot be parameterized.

```py
from typing_extensions import LiteralString

a: LiteralString[str]  # error: [invalid-type-form]
b: LiteralString["foo"]  # error: [invalid-type-form]
```

### As a base class

Subclassing `LiteralString` leads to a runtime error.

```py
from typing_extensions import LiteralString

class C(LiteralString): ...  # error: [invalid-base]
```

## Inference

### Common operations

```py
from typing_extensions import LiteralString

foo: LiteralString = "foo"
reveal_type(foo)  # revealed: Literal["foo"]

bar: LiteralString = "bar"
reveal_type(foo + bar)  # revealed: Literal["foobar"]

baz: LiteralString = "baz"
baz += foo
reveal_type(baz)  # revealed: Literal["bazfoo"]

qux = (foo, bar)
reveal_type(qux)  # revealed: tuple[Literal["foo"], Literal["bar"]]

# TODO: Infer "LiteralString"
reveal_type(foo.join(qux))  # revealed: @Todo(return type of overloaded function)

template: LiteralString = "{}, {}"
reveal_type(template)  # revealed: Literal["{}, {}"]
# TODO: Infer `LiteralString`
reveal_type(template.format(foo, bar))  # revealed: @Todo(return type of overloaded function)
```

### Assignability

`Literal[""]` is assignable to `LiteralString`, and `LiteralString` is assignable to `str`, but not
vice versa.

```py
from typing_extensions import Literal, LiteralString

def _(flag: bool):
    foo_1: Literal["foo"] = "foo"
    bar_1: LiteralString = foo_1  # fine

    foo_2 = "foo" if flag else "bar"
    reveal_type(foo_2)  # revealed: Literal["foo", "bar"]
    bar_2: LiteralString = foo_2  # fine

    foo_3: LiteralString = "foo" * 1_000_000_000
    bar_3: str = foo_2  # fine

    baz_1: str = repr(object())
    qux_1: LiteralString = baz_1  # error: [invalid-assignment]

    baz_2: LiteralString = "baz" * 1_000_000_000
    qux_2: Literal["qux"] = baz_2  # error: [invalid-assignment]

    baz_3 = "foo" if flag else 1
    reveal_type(baz_3)  # revealed: Literal["foo", 1]
    qux_3: LiteralString = baz_3  # error: [invalid-assignment]
```

### Narrowing

```py
from typing_extensions import LiteralString

lorem: LiteralString = "lorem" * 1_000_000_000

reveal_type(lorem)  # revealed: LiteralString

if lorem == "ipsum":
    reveal_type(lorem)  # revealed: Literal["ipsum"]

reveal_type(lorem)  # revealed: LiteralString

if "" < lorem == "ipsum":
    reveal_type(lorem)  # revealed: Literal["ipsum"]
```

## `typing.LiteralString`

`typing.LiteralString` is only available in Python 3.11 and later:

```toml
[environment]
python-version = "3.11"
```

```py
from typing import LiteralString

x: LiteralString = "foo"

def f():
    reveal_type(x)  # revealed: LiteralString
```

[1]: https://typing.readthedocs.io/en/latest/spec/literal.html#literalstring
