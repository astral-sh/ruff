# `LiteralString`

`LiteralString` represents a string that is either defined directly within the source code or is
made up of such components.

Parts of the testcases defined here were adapted from [the specification's examples][1].

## Usages

### Valid places

It can be used anywhere a type is accepted:

```py
from typing import (
    Annotated,
    ClassVar,
    Final,
    Literal,
    LiteralString,
    NotRequired,
    Protocol,
    ReadOnly,
    Required,
    TypedDict,
    TypeVar,
)

TA: TypeAlias = LiteralString

T1 = TypeVar("T1", bound=LiteralString)

variable_annotation: LiteralString

class Foo:
    # fine: Second arguments and later must be ignored.
    annotated: Annotated[int, Literal[LiteralString]]
```

### Within `Literal`

`LiteralString` cannot be used within `Literal`:

```py
from typing import Literal, LiteralString

bad_union: Literal["hello", LiteralString]  # error: [invalid-literal-parameter]
bad_nesting: Literal[LiteralString]  # error: [invalid-literal-parameter]
```

### Parametrized

`LiteralString` cannot be parametrized.

```py
from typing import LiteralString

a: LiteralString[str]  # error: [invalid-type-parameter]
b: LiteralString["foo"]  # error: [invalid-type-parameter]
```

### As a base class

Subclassing `LiteralString` leads to a runtime error.

```py
from typing import LiteralString

class C(LiteralString): ...  # error: [invalid-base]
```

## Inference

### Common operations

```py
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
reveal_type(foo.join(qux))  # revealed: @Todo(call todo)

template: LiteralString = "{}, {}"
reveal_type(template)  # revealed: Literal["{}, {}"]
# TODO: Infer "foo, bar"
reveal_type(template.format(foo, bar))  # revealed: @Todo(call todo)
```

### Assignability

`Literal[""]` is assignable to `LiteralString`, and `LiteralString` is assignable to `str`, but not
vice versa.

```py
def coinflip() -> bool:
    return True

foo_1: Literal["foo"] = "foo"
bar_1: LiteralString = foo_1  # fine

foo_2 = "foo" if coinflip() else "bar"
reveal_type(foo_2)  # revealed: Literal["foo", "bar"]
bar_2: LiteralString = foo_2  # fine

foo_3: LiteralString = "foo" * 1_000_000_000
bar_3: str = foo_2  # fine

baz_1: str = str()
qux_1: LiteralString = baz_1  # error: [invalid-assignment]

baz_2: LiteralString = "baz" * 1_000_000_000
qux_2: Literal["qux"] = baz_2  # error: [invalid-assignment]

baz_3 = "foo" if coinflip() else 1
reveal_type(baz_3)  # revealed: Literal["foo"] | Literal[1]
qux_3: LiteralString = baz_3  # error: [invalid-assignment]
```

### Narrowing

```py
lorem: LiteralString = "lorem" * 1_000_000_000
reveal_type(lorem)  # revealed: LiteralString

if lorem == "ipsum":
    reveal_type(lorem)  # revealed: Literal["ipsum"]

if "" < lorem == "ipsum":
    reveal_type(lorem)  # revealed: Literal["ipsum"]
```

[1]: https://typing.readthedocs.io/en/latest/spec/literal.html#literalstring
