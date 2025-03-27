# Decorators

## Basic example

```py
def custom_decorator(f) -> int:
    return 1

@custom_decorator
def f(x): ...

reveal_type(f)  # revealed: int
```

## Multiple decorators

```py
def maps_to_str(f) -> str:
    return "a"

def maps_to_int(f) -> int:
    return 1

def maps_to_bytes(f) -> bytes:
    return b"a"

@maps_to_str
@maps_to_int
@maps_to_bytes
def f(x): ...

reveal_type(f)  # revealed: str
```

## Unknown decorator

```py
# error: [unresolved-reference] "Name `unknown_decorator` used when not defined"
@unknown_decorator
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

## Non-callable decorator

```py
non_callable = 1

# TODO: Emit a diagnostic that `non_callable` is not callable.
@non_callable
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

## Wrong signature

### Wrong argument type

```py
def wrong_signature(f: int) -> str:
    return "a"

# TODO: Emit a diagnostic that `wrong_signature` does not accept a function.
@wrong_signature
def f(x): ...

reveal_type(f)  # revealed: Unknown
```

### Wrong number of arguments

```py
def wrong_signature(f, g) -> str:
    return "a"

# TODO: Emit a diagnostic that `wrong_signature` is not callable with a single argument.
@wrong_signature
def f(x): ...

reveal_type(f)  # revealed: Unknown
```
