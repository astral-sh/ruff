# Semantic syntax error diagnostics

## `async` comprehensions in synchronous comprehensions

### Python 3.10

<!-- snapshot-diagnostics -->

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
    # error: 19 [invalid-syntax] "cannot use an asynchronous comprehension outside of an asynchronous function on Python 3.10 (syntax was added in 3.11)"
    return {n: [x async for x in elements(n)] for n in range(3)}
```

If all of the comprehensions are `async`, on the other hand, the code was still valid:

```py
async def test():
    return [[x async for x in elements(n)] async for n in range(3)]
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

## Invalid annotation

This one might be a bit redundant with the `invalid-type-form` error.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

# error: [invalid-type-form] "Named expressions are not allowed in type expressions"
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
        ...
```

## `return`, `yield`, `yield from`, and `await` outside function

```py
# error: [invalid-syntax] "`return` statement outside of a function"
return

# error: [invalid-syntax] "`yield` statement outside of a function"
yield

# error: [invalid-syntax] "`yield from` statement outside of a function"
yield from []

# error: [invalid-syntax] "`await` statement outside of a function"
# error: [invalid-syntax] "`await` outside of an asynchronous function"
await 1

def f():
    # error: [invalid-syntax] "`await` outside of an asynchronous function"
    await 1
```

Generators are evaluated lazily, so `await` is allowed, even outside of a function.

```py
async def g():
    yield 1

(x async for x in g())
```

## `await` outside async function

This error includes `await`, `async for`, `async with`, and `async` comprehensions.

```python
async def elements(n):
    yield n

def _():
    # error: [invalid-syntax] "`await` outside of an asynchronous function"
    await 1
    # error: [invalid-syntax] "`async for` outside of an asynchronous function"
    async for _ in elements(1):
        ...
    # error: [invalid-syntax] "`async with` outside of an asynchronous function"
    async with elements(1) as x:
        ...
    # error: [invalid-syntax] "cannot use an asynchronous comprehension outside of an asynchronous function on Python 3.9 (syntax was added in 3.11)"
    # error: [invalid-syntax] "asynchronous comprehension outside of an asynchronous function"
    [x async for x in elements(1)]
```

## Load before `global` declaration

This should be an error, but it's not yet.

TODO implement `SemanticSyntaxContext::global`

```py
def f():
    x = 1
    global x
```
