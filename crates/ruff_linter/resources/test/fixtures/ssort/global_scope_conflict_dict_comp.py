def f():
    g()
    return {g: 1 for g in range(10)}
def g():
    pass
