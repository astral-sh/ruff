# `@no_type_check`

> If a type checker supports the `no_type_check` decorator for functions, it should suppress all type errors for the def statement and its body including any nested functions or classes. It should also ignore all parameter and return type annotations and treat the function as if it were unannotated.
> [source](https://typing.readthedocs.io/en/latest/spec/directives.html#no-type-check)

## Error in the function body

```py
from typing import no_type_check

@no_type_check
def test() -> int: 
    return a + 5
```

## Error in nested function

```py
from typing import no_type_check

@no_type_check
def test() -> int:
    def nested(): 
        return a + 5
```

## Error in nested class

```py
from typing import no_type_check

@no_type_check
def test() -> int:
    class Nested:
        def inner(self):
            return a + 5
```

## Error in decorator

Both MyPy and Pyright flag the `unknown_decorator` but we don't. 

```py
from typing import no_type_check

@unknown_decorator
@no_type_check
def test() -> int:
    return a + 5
```

## Error in default value

```py
from typing import no_type_check

@no_type_check
def test(a: int = "test"): 
    return x + 5
```

## Error in return value position

```py
from typing import no_type_check

@no_type_check
def test() -> Undefined: 
    return x + 5
```

## `no_type_check` on classes isn't supported

Similar to Pyright, Red Knot does not support `no_type_check` annotations on classes.

```py
from typing import no_type_check

@no_type_check
class Test:
    def test(self):
        return a + 5  # error: [unresolved-reference]
```

## `type: ignore` comments in `@no_type_check` blocks

```py
from typing import no_type_check

@no_type_check
def test():
    #  error: [unused-ignore-comment]
    return x + 5  # knot: ignore[unresolved-reference]
```