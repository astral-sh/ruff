def f():
    return a()
def a():
    return b()
def b():
    return f()

