# `LiteralString`

`LiteralString` represents a string that is either defined directly within the source code or is
made up of such components.

Parts of the testcases defined here were adapted from [the specification's examples][1].

## Usages

### Valid places

It can be used anywhere a type is accepted:

```py
from typing_extensions import LiteralString

def _(x: LiteralString):
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

<!-- snapshot-diagnostics -->

```py
from typing_extensions import LiteralString

# error: [invalid-type-form]
a: LiteralString[str]

# error: [invalid-type-form]
b: LiteralString["foo"]
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

def _(literal_a: LiteralString, literal_b: LiteralString, a_str: str):
    # Addition
    reveal_type(literal_a + literal_b)  # revealed: LiteralString
    reveal_type(literal_a + a_str)  # revealed: str
    reveal_type(a_str + literal_a)  # revealed: str

    # In-place addition
    combined_literal = literal_a
    combined_literal += literal_b
    reveal_type(combined_literal)  # revealed: LiteralString
    combined_non_literal1 = literal_a
    combined_non_literal1 += a_str
    reveal_type(combined_non_literal1)  # revealed: str
    combined_non_literal2 = a_str
    combined_non_literal2 += literal_a
    reveal_type(combined_non_literal2)  # revealed: str

    # Join
    reveal_type(literal_a.join(("abc", "foo", literal_a, literal_b)))  # revealed: LiteralString
    reveal_type(a_str.join(("abc", "foo", literal_a, literal_b)))  # revealed: str
    reveal_type(literal_a.join(("abc", "foo", a_str)))  # revealed: str

    # .format(…)
    reveal_type("{}, {}".format(literal_a, literal_b))  # revealed: LiteralString
    reveal_type("{}, {}".format(literal_a, a_str))  # revealed: str

    # f-string
    reveal_type(f"{literal_a} {literal_b}")  # revealed: LiteralString
    reveal_type(f"{literal_a} {a_str}")  # revealed: str

    # Repetition
    reveal_type(literal_a * 10)  # revealed: LiteralString
```

### Assignability

`Literal["abc"]` is assignable to `LiteralString`, and `LiteralString` is assignable to `str`, but
not vice versa.

```py
from typing_extensions import Literal, LiteralString
from ty_extensions import static_assert, is_assignable_to

static_assert(is_assignable_to(Literal[""], LiteralString))
static_assert(is_assignable_to(Literal["abc"], LiteralString))
static_assert(is_assignable_to(Literal["abc", "def"], LiteralString))

static_assert(not is_assignable_to(LiteralString, Literal[""]))
static_assert(not is_assignable_to(LiteralString, Literal["abc"]))
static_assert(not is_assignable_to(LiteralString, Literal["abc", "def"]))

static_assert(is_assignable_to(LiteralString, str))

static_assert(not is_assignable_to(str, LiteralString))
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

def _(x: LiteralString):
    reveal_type(x)  # revealed: LiteralString
```

[1]: https://typing.python.org/en/latest/spec/literal.html#literalstring
