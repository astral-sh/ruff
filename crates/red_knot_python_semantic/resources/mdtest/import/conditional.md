# Conditional imports

## Maybe unbound

```py path=maybe_unbound.py
def coinflip() -> bool:
    return True

if coinflip():
    y = 3

x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

```py
# error: [possibly-unbound-import] "Member `y` of module `maybe_unbound` is possibly unbound"
from maybe_unbound import x, y

reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: Literal[3]
```

## Maybe unbound annotated

```py path=maybe_unbound_annotated.py
def coinflip() -> bool:
    return True

if coinflip():
    y: int = 3

x = y  # error: [possibly-unresolved-reference]

# revealed: Literal[3]
reveal_type(x)

# revealed: Literal[3]
# error: [possibly-unresolved-reference]
reveal_type(y)
```

Importing an annotated name prefers the declared type over the inferred type:

```py
# error: [possibly-unbound-import] "Member `y` of module `maybe_unbound_annotated` is possibly unbound"
from maybe_unbound_annotated import x, y

reveal_type(x)  # revealed: Literal[3]
reveal_type(y)  # revealed: int
```

## Maybe undeclared

Importing a possibly undeclared name still gives us its declared type:

```py path=maybe_undeclared.py
def coinflip() -> bool:
    return True

if coinflip():
    x: int
```

```py
from maybe_undeclared import x

reveal_type(x)  # revealed: int
```

## Reimport

```py path=c.py
def f(): ...
```

```py path=b.py
def coinflip() -> bool:
    return True

if coinflip():
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
def coinflip() -> bool:
    return True

if coinflip():
    from c import x
else:
    x = 1
```

```py
from b import x

reveal_type(x)  # revealed: int
```
