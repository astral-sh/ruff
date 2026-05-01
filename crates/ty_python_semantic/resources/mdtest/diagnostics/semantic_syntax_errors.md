# Semantic syntax error diagnostics

## `async` comprehensions in synchronous comprehensions

### Python 3.10

Before Python 3.11, `async` comprehensions could not be used within outer sync comprehensions, even
within an `async` function ([CPython issue](https://github.com/python/cpython/issues/77527)):

```toml
[environment]
python-version = "3.10"
```

```py
async def elements(n):
    yield n

async def f():
    # snapshot: invalid-syntax
    return {n: [x async for x in elements(n)] for n in range(3)}
```

```snapshot
error[invalid-syntax]: cannot use an asynchronous comprehension inside of a synchronous comprehension on Python 3.10 (syntax was added in 3.11)
 --> src/mdtest_snippet.py:6:19
  |
6 |     return {n: [x async for x in elements(n)] for n in range(3)}
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

If all of the comprehensions are `async`, on the other hand, the code was still valid:

```py
async def test():
    # snapshot: not-iterable
    return [[x async for x in elements(n)] async for n in range(3)]
```

```snapshot
error[not-iterable]: Object of type `range` is not async-iterable
 --> src/mdtest_snippet.py:9:59
  |
9 |     return [[x async for x in elements(n)] async for n in range(3)]
  |                                                           ^^^^^^^^
  |
info: It has no `__aiter__` method
```

These are a couple of tricky but valid cases to check that nested scope handling is wired up
correctly in the `SemanticSyntaxContext` trait:

```py
async def f():
    [x for x in [1]] and [x async for x in elements(1)]

async def f():
    def g():
        pass
    [x async for x in elements(1)]
```

### Python 3.11

All of these same examples are valid after Python 3.11:

```toml
[environment]
python-version = "3.11"
```

```py
async def elements(n):
    yield n

async def f():
    return {n: [x async for x in elements(n)] for n in range(3)}
```

## Late `__future__` import

```py
from collections import namedtuple

# error: [invalid-syntax] "__future__ imports must be at the top of the file"
from __future__ import print_function
```

## Lazy `__future__` imports are not future imports

```toml
[environment]
python-version = "3.15"
```

```py
# error: [invalid-syntax] "lazy from __future__ import is not allowed"
lazy from __future__ import annotations

# error: [invalid-syntax] "__future__ imports must be at the top of the file"
from __future__ import generator_stop
```

## Invalid annotation

This one might be a bit redundant with the `invalid-type-form` error.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

# error: [invalid-type-form] "Named expressions are not allowed in return type annotations"
# error: [invalid-syntax] "named expression cannot be used within a type annotation"
def f() -> (y := 3): ...
```

## Duplicate `match` key

```toml
[environment]
python-version = "3.10"
```

```py
match 2:
    # error: [invalid-syntax] "mapping pattern checks duplicate key `"x"`"
    case {"x": 1, "x": 2}:
        pass
```

## Duplicate `match` class attribute

Attribute names in class patterns must be unique:

```toml
[environment]
python-version = "3.10"
```

```py
class Point:
    pass

obj = Point()
match obj:
    # error: [invalid-syntax] "attribute name `x` repeated in class pattern"
    case Point(x=1, x=2):
        pass
```

## `return`, `yield`, `yield from`, and `await` outside function

```py
class C:
    def __await__(self): ...

# error: [invalid-syntax] "`return` statement outside of a function"
return

# error: [invalid-syntax] "`yield` statement outside of a function"
yield

# error: [invalid-syntax] "`yield from` statement outside of a function"
yield from []

# error: [invalid-syntax] "`await` outside of an asynchronous function"
await C()

def f():
    # error: [invalid-syntax] "`await` outside of an asynchronous function"
    await C()

(await cor async for cor in f())  # ok
(await cor for cor in f())  # ok
([await c for c in cor] async for cor in f())  # ok
```

Generators are evaluated lazily, so `await` is allowed, even outside of a function.

```py
async def g():
    yield 1

(x async for x in g())
```

## Rebound comprehension variable

Walrus operators cannot rebind variables already in use as iterators:

```py
# error: [invalid-syntax] "assignment expression cannot rebind comprehension variable"
[x := 2 for x in range(10)]

# error: [invalid-syntax] "assignment expression cannot rebind comprehension variable"
{y := 5 for y in range(10)}

# error: [invalid-syntax] "assignment expression cannot rebind comprehension variable"
[(a := 0) for a in range(3)]
# error: [unresolved-reference]
reveal_type(a)  # revealed: Unknown

# error: [invalid-syntax] "assignment expression cannot rebind comprehension variable"
[i for i in range(5) if (i := 0)]
# error: [unresolved-reference]
reveal_type(i)  # revealed: Unknown

[x for x in range(3) if (lambda: (x := 1))()]
# error: [unresolved-reference]
reveal_type(x)  # revealed: Unknown
```

## Walrus in invalid comprehension contexts

```py
class C:
    # error: [invalid-syntax] "assignment expression within a comprehension cannot be used in a class body"
    [(x := y) for y in range(3)]
    # error: [unresolved-reference]
    reveal_type(x)  # revealed: Unknown

def returns_list() -> list[int]:
    return [1, 2, 3]

# error: [invalid-syntax] "assignment expression cannot be used in a comprehension iterable expression"
[x for x in (y := returns_list())]
# error: [unresolved-reference]
reveal_type(y)  # revealed: Unknown
```

## Multiple case assignments

Variable names in pattern matching must be unique within a single pattern:

```toml
[environment]
python-version = "3.10"
```

```py
x = [1, 2]
match x:
    # error: [invalid-syntax] "multiple assignments to name `a` in pattern"
    case [a, a]:
        pass
    case _:
        pass

d = {"key": "value"}
match d:
    # error: [invalid-syntax] "multiple assignments to name `b` in pattern"
    case {"key": b, "other": b}:
        pass
```

## Duplicate type parameter

Type parameter names must be unique in a generic class or function definition:

```toml
[environment]
python-version = "3.12"
```

```py
# error: [invalid-syntax] "duplicate type parameter"
class C[T, T]:
    pass

# error: [invalid-syntax] "duplicate type parameter"
def f[X, Y, X]():
    pass
```

## Invalid star expression

Star expressions can't be used in certain contexts:

```py
def func():
    # error: [invalid-syntax] "Starred expression cannot be used here"
    return *[1, 2, 3]

def gen():
    # error: [invalid-syntax] "Starred expression cannot be used here"
    yield *[1, 2, 3]

# error: [invalid-syntax] "Starred expression cannot be used here"
for *x in range(10):
    pass

# error: [invalid-syntax] "Starred expression cannot be used here"
for x in *range(10):
    pass
```

## Irrefutable case pattern

Irrefutable patterns, i.e. wildcard or capture patterns, must be the last case in a match statement.
Following case statements are unreachable.

```toml
[environment]
python-version = "3.12"
```

```py
value = 5

match value:
    # error: [invalid-syntax] "wildcard makes remaining patterns unreachable"
    case _:  # Irrefutable wildcard pattern
        pass
    case 5:
        pass

match value:
    # error: [invalid-syntax] "name capture `variable` makes remaining patterns unreachable"
    case variable:  # Irrefutable capture pattern
        pass
    case 10:
        pass
```

## Single starred assignment

Starred assignment targets cannot appear by themselves. They must be in the context of a list or
tuple.

```py
# error: [invalid-syntax] "starred assignment target must be in a list or tuple"
*a = [1, 2, 3, 4]
```

## Write to debug

The special Python builtin `__debug__` should not be modified.

```toml
[environment]
python-version = "3.12"
```

```py
# error: [invalid-syntax] "cannot assign to `__debug__`"
__debug__ = False

# error: [invalid-syntax] "cannot assign to `__debug__`"
def process(__debug__):
    pass

# error: [invalid-syntax] "cannot assign to `__debug__`"
class Generic[__debug__]:
    pass
```

## Invalid expression

Certain expressions like `yield` or inlined walrus assignments are not valid in specific contexts.

```toml
[environment]
python-version = "3.12"
```

```py
def _():
    # error: [invalid-type-form] "`yield` expressions are not allowed in type expressions"
    # error: [invalid-syntax] "yield expression cannot be used within a TypeVar bound"
    type X[T: (yield 1)] = int

def _():
    # error: [invalid-type-form] "`yield` expressions are not allowed in type alias values"
    # error: [invalid-syntax] "yield expression cannot be used within a type alias"
    type Y = (yield 1)

# error: [invalid-type-form] "Named expressions are not allowed in return type annotations"
# error: [invalid-syntax] "named expression cannot be used within a generic definition"
def f[T](x: int) -> (y := 3):
    return x

def _():
    # error: [invalid-syntax] "yield expression cannot be used within a generic definition"
    class C[T]((yield from [object])):
        pass
```

## `await` outside async function

This error includes `await`, `async for`, `async with`, and `async` comprehensions.

```py
async def elements(n):
    yield n

def _():
    # error: [invalid-syntax] "`await` outside of an asynchronous function"
    await elements(1)

    # error: [invalid-syntax] "`async for` outside of an asynchronous function"
    async for _ in elements(1):
        pass
    # error: [invalid-syntax] "`async with` outside of an asynchronous function"
    async with elements(1) as x:
        pass
    # error: [invalid-syntax] "asynchronous comprehension outside of an asynchronous function"
    [x async for x in elements(1)]
```

## Load before `global` declaration

```py
x: int

def f():
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
```

## `break` and `continue` outside a loop

```py
break  # snapshot: invalid-syntax
continue  # snapshot: invalid-syntax

for x in range(42):
    break  # fine
    continue  # fine

    def _():
        break  # snapshot: invalid-syntax
        continue  # snapshot: invalid-syntax

    class Fine:
        # this is invalid syntax despite it being in an eager-nested scope!
        break  # snapshot: invalid-syntax
        continue  # snapshot: invalid-syntax
```

```snapshot
error[invalid-syntax]: `break` outside loop
 --> src/mdtest_snippet.py:1:1
  |
1 | break  # snapshot: invalid-syntax
  | ^^^^^
  |


error[invalid-syntax]: `continue` outside loop
 --> src/mdtest_snippet.py:2:1
  |
2 | continue  # snapshot: invalid-syntax
  | ^^^^^^^^
  |


error[invalid-syntax]: `break` outside loop
 --> src/mdtest_snippet.py:9:9
  |
9 |         break  # snapshot: invalid-syntax
  |         ^^^^^
  |


error[invalid-syntax]: `continue` outside loop
  --> src/mdtest_snippet.py:10:9
   |
10 |         continue  # snapshot: invalid-syntax
   |         ^^^^^^^^
   |


error[invalid-syntax]: `break` outside loop
  --> src/mdtest_snippet.py:14:9
   |
14 |         break  # snapshot: invalid-syntax
   |         ^^^^^
   |


error[invalid-syntax]: `continue` outside loop
  --> src/mdtest_snippet.py:15:9
   |
15 |         continue  # snapshot: invalid-syntax
   |         ^^^^^^^^
   |
```

## name cannot refer to a parameter and a global variable

```py
a = None

def f(a):
    global a  # snapshot: invalid-syntax

def g(a):
    if True:
        global a  # snapshot: invalid-syntax

def h(a):
    def inner():
        global a

def i(a):
    try:
        global a  # snapshot: invalid-syntax
    except Exception:
        pass

def f(a):
    a = 1
    global a  # snapshot: invalid-syntax

def f(a):
    a = 1
    a = 2
    global a  # snapshot: invalid-syntax

def f(a):
    class Inner:
        global a  # ok
```

```snapshot
error[invalid-syntax]: name `a` cannot refer to a parameter and a global variable
 --> src/mdtest_snippet.py:4:12
  |
4 |     global a  # snapshot: invalid-syntax
  |            ^
  |


error[invalid-syntax]: name `a` cannot refer to a parameter and a global variable
 --> src/mdtest_snippet.py:8:16
  |
8 |         global a  # snapshot: invalid-syntax
  |                ^
  |


error[invalid-syntax]: name `a` cannot refer to a parameter and a global variable
  --> src/mdtest_snippet.py:16:16
   |
16 |         global a  # snapshot: invalid-syntax
   |                ^
   |


error[invalid-syntax]: name `a` cannot refer to a parameter and a global variable
  --> src/mdtest_snippet.py:22:12
   |
22 |     global a  # snapshot: invalid-syntax
   |            ^
   |


error[invalid-syntax]: name `a` cannot refer to a parameter and a global variable
  --> src/mdtest_snippet.py:27:12
   |
27 |     global a  # snapshot: invalid-syntax
   |            ^
   |
```
