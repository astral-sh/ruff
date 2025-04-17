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
