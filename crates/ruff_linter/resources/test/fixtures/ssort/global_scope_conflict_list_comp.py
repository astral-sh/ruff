def f():
    g()
    return [g for g in range(10)]
def g():
    pass
