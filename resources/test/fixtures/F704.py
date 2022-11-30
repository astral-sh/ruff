def f() -> int:
    yield 1


class Foo:
    yield 2


yield 3
yield from 3
await f()
