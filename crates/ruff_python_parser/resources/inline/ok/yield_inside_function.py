def f():
    yield 1
    yield from 1
    await 1
    yield
    [(yield x) for x in range(3)]
