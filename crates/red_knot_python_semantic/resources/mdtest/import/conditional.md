# Conditional imports

## Maybe unbound

```py path=maybe_unbound.py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    y = 3

x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

```py
from maybe_unbound import x, y

reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: Literal[3]
```

## Maybe unbound annotated

```py path=maybe_unbound_annotated.py
def bool_instance() -> bool:
    return True

flag = bool_instance()

if flag:
    y: int = 3
x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

Importing an annotated name prefers the declared type over the inferred type:

```py
from maybe_unbound_annotated import x, y

reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: int
```

## Reimport

```py path=c.py
def f(): ...
```

```py path=b.py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    from c import f
else:

    def f(): ...
```

```py
from b import f

# TODO: We should disambiguate in such cases, showing `Literal[b.f, c.f]`.
reveal_type(f)  # revealed: Literal[f, f]
```

## Reimport with stub declaration

When we have a declared type in one path and only an inferred-from-definition type in the other, we
should still be able to unify those:

```py path=c.pyi
x: int
```

```py path=b.py
def bool_instance() -> bool:
    return True

flag = bool_instance()
if flag:
    from c import x
else:
    x = 1
```

```py
from b import x

reveal_type(x)  # revealed: int
```
