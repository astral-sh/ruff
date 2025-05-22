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
