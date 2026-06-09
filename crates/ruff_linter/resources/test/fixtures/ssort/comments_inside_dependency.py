def a():
    # depends on b()
    return b() + 1

def b():
    return True

def c():
    # depends on a()
    return a() + 1

