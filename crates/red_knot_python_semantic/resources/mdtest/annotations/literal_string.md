# `LiteralString`

`LiteralString` signifies a strings that is either
defined directly within the source code or is made up of such components.

Part of the testcases defined here were adapted from [the specification's examples][1].

[1]: https://typing.readthedocs.io/en/latest/spec/literal.html#literalstring

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
    TypeAlias,
    # TODO: Blocking on `sys.version_info` support.
    # See `issubclass.md`, section "Handling of `None`"
    TypeAliasType,  # error: [possibly-unbound-import]
    TypeVar,
)

Old: TypeAlias = LiteralString
type New = LiteralString
Backported = TypeAliasType("Backported", LiteralString)

T1 = TypeVar("T1", bound=LiteralString)
T2 = TypeVar("T2", bound=Old)
T3 = TypeVar("T3", bound=New)
T4 = TypeVar("T4", bound=Backported)

variable_annotation_1: LiteralString
variable_annotation_2: Old
variable_annotation_3: New
variable_annotation_4: Backported

type_argument_1: list[LiteralString]
type_argument_2: dict[Old, New]
type_argument_3: set[Backported]

type TA1[LS: LiteralString] = Old
type TA2[LS: Old] = New
type TA3[LS: New] = Backported
type TA4[LS: Backported] = LiteralString

def my_function(literal_string: LiteralString, *args: Old, **kwargs: New) -> Backported: ...

class Foo:
    my_attribute: LiteralString
    class_var: ClassVar[Old] = "foo"
    final: Final[New] = "bar"
    annotated_class_var: Annotated[
        ClassVar[Backported],
        Literal[LiteralString]  # Second arguments and later must be ignored.
    ] = "foobar"

# TODO: Support new-style generic classes
# error: [invalid-base]
class PEP695[L: LiteralString](Protocol):
    def f[S: Old](self: L | S | New) -> Backported: ...
    #                   ^^^^^^^^^^^ This is valid, as the class is a protocol.

class GenericParameter(PEP695[LiteralString]):
    ...

# TODO: Support TypedDict
class TD(TypedDict):  # error: [invalid-base]
    normal: LiteralString
    readonly: ReadOnly[Old]
    required: Required[New]
    not_required: NotRequired[Backported]
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
# TODO: See above.
# error: [possibly-unbound-import]
from typing import LiteralString, TypeAlias, TypeAliasType

Old: TypeAlias = LiteralString
type New = LiteralString
Backported = TypeAliasType("Backported", LiteralString)

a: LiteralString[str]  # error: [invalid-type-parameter]
b: LiteralString["foo"]  # error: [invalid-type-parameter]

c: Old[str]  # error: [invalid-type-parameter]
d: Old["foo"]  # error: [invalid-type-parameter]

# TODO: Emit invalid-type-parameter for the following
e: New[str]
f: New["int"]

g: Backported[str]
h: Backported["int"]

# fine: TypeAliasType instances are subscriptable.
# These are not required to have a meaning outside annotation contexts.
New[str]
New["int"]
Backported[str]
Backported["int"]
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
reveal_type(foo.join(qux))  # revealed: @Todo(call type)

template: LiteralString = "{}, {}"
reveal_type(template)  # revealed: Literal["{}, {}"]
# TODO: Infer "foo, bar"
reveal_type(template.format(foo, bar))  # revealed: @Todo(call type)
```

### Compatibility

`Literal["", ...]` is compatible with `LiteralString`,
`LiteralString` is compatible with `str`, but not vice versa.

```py
foo_1: Literal["foo"] = "foo"
bar_1: LiteralString = foo_1  # fine

if bool():
    foo_2 = "foo"
else:
    foo_2 = "bar"
reveal_type(foo_2)  # revealed: Literal["foo", "bar"]
bar_2: LiteralString = foo_2  # fine

foo_3: LiteralString = "foo" * 1_000_000_000
bar_3: str = foo_2  # fine

baz_1: str = str()
qux_1: LiteralString = baz_1  # error: [invalid-assignment]

baz_2: LiteralString = "baz" * 1_000_000_000
qux_2: Literal["qux"] = baz_2  # error: [invalid-assignment]

if bool():
    baz_3 = "foo"
else:
    baz_3 = 1
reveal_type(baz_3)  # revealed: Literal["foo"] | Literal[1]
qux_3: LiteralString = baz_3  # error: [invalid-assignment]
```

### Narrowing

```py
lorem: LiteralString = "lorem" * 1_000_000_000
reveal_type(lorem)  # revealed: LiteralString

if lorem == "ipsum":
    reveal_type(lorem)  # revealed: Literal["ipsum"]
```
