# Static binary operations using `in`

## Basic functionality

This demonstrates type inference support for `<str-literal> in <tuple>`:

```py
from ty_extensions import static_assert

static_assert("foo" in ("quux", "foo", "baz"))
static_assert("foo" not in ("quux", "bar", "baz"))
```

## With variables

```py
from ty_extensions import static_assert

x = ("quux", "foo", "baz")
static_assert("foo" in x)

x = ("quux", "bar", "baz")
static_assert("foo" not in x)
```

## Statically unknown results in a `bool`

```py
def _(a: str, b: str):
    reveal_type("foo" in (a, b))  # revealed: bool
```

## Values being unknown doesn't mean the result is unknown

For example, when the types are completely disjoint:

```py
from ty_extensions import static_assert

def _(a: int, b: int):
    static_assert("foo" not in (a, b))
```

## Failure cases

```py
# We don't support byte strings.
reveal_type(b"foo" not in (b"quux", b"foo", b"baz"))  # revealed: bool
```
