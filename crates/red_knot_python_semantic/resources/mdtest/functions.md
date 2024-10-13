# Functions

## Call Expression

### Simple

Calling a simple function that returns an integer should infer the correct return type.

```py
def get_int() -> int:
    return 42

x = get_int()
reveal_type(x)  # revealed: int
```

### TODO: Async

NOTE: This is a TODO because we don't yet support `types.CoroutineType`. `x` should be generic `Coroutine[Any, Any, int]`!

```py
async def get_int_async() -> int:
    return 42

x = get_int_async()
reveal_type(x)  # revealed: @Todo
```

### TODO: Decorated

A function decorated with a decorator that alters its return type should reflect the decorated type.

TODO: `x` should reveal `int`, as the decorator replaces `bar` with `foo`!

```py
from typing import Callable

def foo() -> int:
    return 42

def decorator(func) -> Callable[[], int]:
    return foo

@decorator
def bar() -> str:
    return 'bar'

x = bar()
reveal_type(x)  # revealed: @Todo
```

### Constructor

Calling a class constructor should return an instance of the class.

```py
class Foo: ...

x = Foo()
reveal_type(x)  # revealed: Foo
```

### Union of return types

A function whose return type is conditionally either an `int` or `str` should be inferred as a union of both.

```py
if flag:
    def f() -> int:
        return 1
else:
    def f() -> str:
        return 'foo'

x = f()
reveal_type(x)  # revealed: int | str
```

### Calling with an unknown union

If one element in a union is unknown or from an unresolved import, the type system should reflect that.

```py
from nonexistent import f # error: [unresolved-import] "Cannot resolve import `nonexistent`"

if flag:
    def f() -> int:
        return 1

x = f()
reveal_type(x)  # revealed: Unknown | int
```

### Non-callable elements in a union

If a variable in a union isn't callable, it should raise an error when called.

```py
if flag:
    f = 1
else:
    def f() -> int:
        return 1

x = f()  # error: "Object of type `Literal[1] | Literal[f]` is not callable (due to union element `Literal[1]`)"
reveal_type(x)  # revealed: Unknown | int
```

### Multiple non-callable elements in a union

When more than one element of a union is not callable, the system should flag both.

```py
if flag:
    f = 1
elif flag2:
    f = 'foo'
else:
    def f() -> int:
        return 1

x = f()  # error: "Object of type `Literal[1] | Literal["foo"] | Literal[f]` is not callable (due to union elements Literal[1], Literal["foo"])"
reveal_type(x)  # revealed: Unknown | int
```

### All non-callable union elements

If none of the elements in a union are callable, the type system should raise an error.

```py
if flag:
    f = 1
else:
    f = 'foo'

x = f()  # error: "Object of type `Literal[1] | Literal["foo"]` is not callable"
reveal_type(x)  # revealed: Unknown
```

### Invalid callable

Attempting to call a non-callable object should trigger an error.

```py
nonsense = 123
x = nonsense()  # error: "Object of type `Literal[123]` is not callable"
```

## Shadowing

### Parameter

In Python, it's valid to reassign or redeclare a function parameter within the body of the function. In this example, the parameter `x` of type `str` is shadowed and reassigned with a new `int` value inside the function. No diagnostics should be generated.

```py path=a.py
def f(x: str):
    x: int = int(x)
```

### Implicit error

In this case, a function `f` is defined, but then shadowed by an assignment to the same name. This implicit shadowing generates a diagnostic because it's unclear whether the shadowing is intentional.

```py path=a.py
def f(): pass
f = 1 # error: "Implicit shadowing of function `f`; annotate to make it explicit if this is intentional"
```

### Explicit shadowing

If the intention is to shadow the function, the variable can be explicitly annotated with a type to indicate the shadowing is intentional. In this case, no diagnostic is generated.

```py path=a.py
def f(): pass
f: int = 1
```
