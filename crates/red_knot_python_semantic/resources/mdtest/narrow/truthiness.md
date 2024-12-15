# Narrowing For Truthiness Checks (`if x` or `if not x`)

## Value Literals

```py
def bool_instance() -> bool:
    return True

def foo() -> Literal[0, -1, True, False, "", "foo", b"", b"bar"] | tuple[()] | None:
    return 0

x = foo()

if x:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"] | Literal[b"bar"]
else:
    reveal_type(x)  # revealed: Literal[0] | Literal[False] | Literal[""] | Literal[b""] | tuple[()] | None

if not x:
    reveal_type(x)  # revealed: Literal[0] | Literal[False] | Literal[""] | Literal[b""] | tuple[()] | None
else:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"] | Literal[b"bar"]

if x and not x:
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["", "foo"] | Literal[b"", b"bar"] | tuple[()] | None

if not (x and not x):
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["", "foo"] | Literal[b"", b"bar"] | tuple[()] | None
else:
    reveal_type(x)  # revealed: Never

if x or not x:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["foo", ""] | Literal[b"bar", b""] | tuple[()] | None
else:
    reveal_type(x)  # revealed: Never

if not (x or not x):
    reveal_type(x)  # revealed: Never
else:
    reveal_type(x)  # revealed: Literal[-1, 0] | bool | Literal["foo", ""] | Literal[b"bar", b""] | tuple[()] | None

if (isinstance(x, int) or isinstance(x, str)) and x:
    reveal_type(x)  # revealed: Literal[-1] | Literal[True] | Literal["foo"]
else:
    reveal_type(x)  # revealed: Literal[b"", b"bar"] | tuple[()] | None | Literal[0] | Literal[False] | Literal[""]
```

## Function Literals

Basically functions are always truthy.

```py
def flag() -> bool:
    return True

def foo(hello: int) -> bytes:
    return b""

x = flag if flag() else foo

if x:
    reveal_type(x)  # revealed: Literal[flag, foo]
else:
    reveal_type(x)  # revealed: Never
```

## Mutable Truthiness

The boolean value of an instance is not always consistent. For example, `__bool__` can be customized
to return random values, or in the case of a `list()`, the result depends on the number of elements
in the list. Therefore, these types should not be narrowed by `if x` or `if not x`.

Also, types may not be Truthy. This is because `__bool__` can be customized via a metaclass.
Although this is a very rare case, we may consider metaclass checks in the future to handle this
more accurately.

```py
def flag() -> bool:
    return True

class A: ...
class B: ...

x = A() if flag() else B()

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

x = int if flag() else str
reveal_type(x)  # revealed: Literal[int, str]

if x:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysFalsy | Literal[str] & ~AlwaysFalsy
else:
    reveal_type(x)  # revealed: Literal[int] & ~AlwaysTruthy | Literal[str] & ~AlwaysTruthy
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
def flag() -> bool:
    return True

x = 0 if flag() else 1
y = "" if flag() else "hello"

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

# TODO: It should be A. We should improve UnionBuilder or IntersectionBuilder.
reveal_type(y)  # revealed: A & ~AlwaysTruthy | A & ~AlwaysFalsy
```
