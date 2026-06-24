def f() -> int:
    yield 1


class Foo:
    yield 2


yield 3
yield from 3
await f()

def _():
    # Invalid yield scopes; but not outside a function
    type X[T: (yield 1)] = int
    type Y = (yield 2)

    # Valid yield scope
    yield 3


# await is valid in any generator, sync or async
(await cor async for cor in f())  # ok
(await cor for cor in f())  # ok

# but not in comprehensions
[await cor async for cor in f()]  # F704
{await cor async for cor in f()}  # F704
{await cor: 1 async for cor in f()}  # F704
[await cor for cor in f()]  # F704
{await cor for cor in f()}  # F704
{await cor: 1 for cor in f()}  # F704

# or in the iterator of an async generator, which is evaluated in the parent
# scope
(cor async for cor in await f())  # F704
(await cor async for cor in [await c for c in f()])  # F704

# this is also okay because the comprehension is within the generator scope
([await c for c in cor] async for cor in f())  # ok
