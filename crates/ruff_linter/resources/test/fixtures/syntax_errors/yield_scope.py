def outer():
    class C:
        yield 1  # error

    [(yield 1) for x in range(3)]  # error
    ((yield 1) for x in range(3))  # error
    {(yield 1) for x in range(3)}  # error
    {(yield 1): 0 for x in range(3)}  # error
    {0: (yield 1) for x in range(3)}  # error
