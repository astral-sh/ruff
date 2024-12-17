# Narrowing For Truthiness Checks (`if x` or `if not x`)

## Value Literals

```py
def foo() -> Literal[0, -1, True, False, "", "foo", b"", b"bar", None] | tuple[()]:
    return 0

x = foo()

if x:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"] | Literal[b"bar"]
else:
    reveal_type(x)  # revealed: Literal[0] | Literal[False] | Literal[""] | Literal[b""] | None | tuple[()]

if not x:
    reveal_type(x)  # revealed: Literal[0] | Literal[False] | Literal[""] | Literal[b""] | None | tuple[()]
else:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"] | Literal[b"bar"]

if x and not x:
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["", "foo"] | Literal[b"", b"bar"] | None | tuple[()]

if not (x and not x):
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["", "foo"] | Literal[b"", b"bar"] | None | tuple[()]
else:
    reveal_type(x)  # revealed: Never

if x or not x:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["foo", ""] | Literal[b"bar", b""] | None | tuple[()]
else:
    reveal_type(x)  # revealed: Never

if not (x or not x):
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["foo", ""] | Literal[b"bar", b""] | None | tuple[()]

if (isinstance(x, int) or isinstance(x, str)) and x:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"]
else:
    reveal_type(x)  # revealed: Literal[b"", b"bar"] | None | tuple[()] | Literal[0] | Literal[False] | Literal[""]
```

## Function Literals

Basically functions are always truthy.

```py
def flag() -> bool:
    return True

def foo(hello: int) -> bytes:
    return b""

def bar(world: str, *args, **kwargs) -> float:
    return 0.0

x = foo if flag() else bar

if x:
    reveal_type(x)  # revealed: Literal[foo, bar]
else:
    reveal_type(x)  # revealed: Never
```

## Mutable Truthiness

### Truthiness of Instances

The boolean value of an instance is not always consistent. For example, `__bool__` can be customized
to return random values, or in the case of a `list()`, the result depends on the number of elements
in the list. Therefore, these types should not be narrowed by `if x` or `if not x`.

```py
class A: ...
class B: ...

def f(x: A | B):
    if x:
        reveal_type(x)  # revealed: A & ~AlwaysFalsy | B & ~AlwaysFalsy
    else:
        reveal_type(x)  # revealed: A & ~AlwaysTruthy | B & ~AlwaysTruthy

    if x and not x:
        reveal_type(x)  # revealed: A & ~AlwaysFalsy & ~AlwaysTruthy | B & ~AlwaysFalsy & ~AlwaysTruthy
    else:
        reveal_type(x)  # revealed: A & ~AlwaysTruthy | B & ~AlwaysTruthy | A & ~AlwaysFalsy | B & ~AlwaysFalsy

    if x or not x:
        reveal_type(x)  # revealed: A & ~AlwaysFalsy | B & ~AlwaysFalsy | A & ~AlwaysTruthy | B & ~AlwaysTruthy
    else:
        reveal_type(x)  # revealed: A & ~AlwaysTruthy & ~AlwaysFalsy | B & ~AlwaysTruthy & ~AlwaysFalsy
```

### Truthiness of Types

Also, types may not be Truthy. This is because `__bool__` can be customized via a metaclass.
Although this is a very rare case, we may consider metaclass checks in the future to handle this
more accurately.

```py
def flag() -> bool:
    return True

x = int if flag() else str
reveal_type(x)  # revealed: Literal[int, str]

if x:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysFalsy | Literal[str] & ~AlwaysFalsy
else:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysTruthy | Literal[str] & ~AlwaysTruthy
```

## Determined Truthiness

Some custom classes can have a boolean value that is consistently determined as either `True` or
`False`, regardless of the instance's state. This is achieved by defining a `__bool__` method that
always returns a fixed value.

These types can always be fully narrowed in boolean contexts, as shown below:

```py
class T:
    def __bool__(self) -> Literal[True]:
        return True

class F:
    def __bool__(self) -> Literal[False]:
        return False

t = T()

if t:
    reveal_type(t)  # revealed: T
else:
    reveal_type(t)  # revealed: Never

f = F()

if f:
    reveal_type(f)  # revealed: Never
else:
    reveal_type(f)  # revealed: F
```

## Narrowing Complex Intersection and Union

```py
class A: ...
class B: ...

def flag() -> bool:
    return True

def instance() -> A | B:
    return A()

def literals() -> Literal[0, 42, "", "hello"]:
    return 42

x = instance()
y = literals()

if isinstance(x, str) and not isinstance(x, B):
    reveal_type(x)  # revealed: A & str & ~B
    reveal_type(y)  # revealed: Literal[0, 42] | Literal["", "hello"]

    z = x if flag() else y

    reveal_type(z)  # revealed: A & str & ~B | Literal[0, 42] | Literal["", "hello"]

    if z:
        reveal_type(z)  # revealed: A & str & ~B & ~AlwaysFalsy | Literal[42] | Literal["hello"]
    else:
        reveal_type(z)  # revealed: A & str & ~B & ~AlwaysTruthy | Literal[0] | Literal[""]
```

## Narrowing Multiple Variables

```py
def f(x: Literal[0, 1], y: Literal["", "hello"]):
    if x and y and not x and not y:
        reveal_type(x)  # revealed: Never
        reveal_type(y)  # revealed: Never
    else:
        # ~(x or not x) and ~(y or not y)
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Literal["", "hello"]

    if (x or not x) and (y and not y):
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Never
    else:
        # ~(x or not x) or ~(y and not y)
        reveal_type(x)  # revealed: Literal[0, 1]
        reveal_type(y)  # revealed: Literal["", "hello"]
```

## ControlFlow Merging

After merging control flows, when we take the union of all constraints applied in each branch, we
should return to the original state.

```py
class A: ...

x = A()

if x and not x:
    y = x
    reveal_type(y)  # revealed: A & ~AlwaysFalsy & ~AlwaysTruthy
else:
    y = x
    reveal_type(y)  # revealed: A & ~AlwaysTruthy | A & ~AlwaysFalsy

# TODO: It should be A. We should improve UnionBuilder or IntersectionBuilder. (issue #15023)
reveal_type(y)  # revealed: A & ~AlwaysTruthy | A & ~AlwaysFalsy
```
