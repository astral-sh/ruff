# Narrowing for checks involving `type(x)`

## `type(x) is C`

```py
class A: ...
class B: ...

def get_a_or_b() -> A | B:
    return A()

x = get_a_or_b()

if type(x) is A:
    reveal_type(x)  # revealed: A
else:
    # It would be wrong to infer `B` here. The type
    # of `x` could be a subclass of `A`, so we need
    # to infer the full union type:
    reveal_type(x)  # revealed: A | B
```

## `type(x) is not C`

```py
class A: ...
class B: ...

def get_a_or_b() -> A | B:
    return A()

x = get_a_or_b()

if type(x) is not A:
    # Same reasoning as above: no narrowing should occur here.
    reveal_type(x)  # revealed: A | B
else:
    reveal_type(x)  # revealed: A
```

## `type(x) == C`, `type(x) != C`

No narrowing can occur for equality comparisons, since there might be a custom `__eq__`
implementation on the metaclass.

TODO: Narrowing might be possible in some cases where the classes themselves are `@final` or their
metaclass is `@final`.

```py
class IsEqualToEverything(type):
    def __eq__(cls, other):
        return True

class A(metaclass=IsEqualToEverything): ...
class B(metaclass=IsEqualToEverything): ...

def get_a_or_b() -> A | B:
    return B()

x = get_a_or_b()

if type(x) == A:
    reveal_type(x)  # revealed: A | B

if type(x) != A:
    reveal_type(x)  # revealed: A | B
```

## No narrowing for custom `type` callable

```py
class A: ...
class B: ...

def type(x):
    return int

def get_a_or_b() -> A | B:
    return A()

x = get_a_or_b()

if type(x) is A:
    reveal_type(x)  # revealed: A | B
else:
    reveal_type(x)  # revealed: A | B
```

## No narrowing for multiple arguments

No narrowing should occur if `type` is used to dynamically create a class:

```py
def get_str_or_int() -> str | int:
    return "test"

x = get_str_or_int()

if type(x, (), {}) is str:
    reveal_type(x)  # revealed: str | int
else:
    reveal_type(x)  # revealed: str | int
```

## No narrowing for keyword arguments

`type` can't be used with a keyword argument:

```py
def get_str_or_int() -> str | int:
    return "test"

x = get_str_or_int()

# TODO: we could issue a diagnostic here
if type(object=x) is str:
    reveal_type(x)  # revealed: str | int
```

## Narrowing if `type` is aliased

```py
class A: ...
class B: ...

alias_for_type = type

def get_a_or_b() -> A | B:
    return A()

x = get_a_or_b()

if alias_for_type(x) is A:
    reveal_type(x)  # revealed: A
```

## Limitations

```py
class Base: ...
class Derived(Base): ...

def get_base() -> Base:
    return Base()

x = get_base()

if type(x) is Base:
    # Ideally, this could be narrower, but there is now way to
    # express a constraint like `Base & ~ProperSubtypeOf[Base]`.
    reveal_type(x)  # revealed: Base
```
