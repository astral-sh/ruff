###
# Errors.
###
def f():
    global X


def f():
    global X

    print(X)


def f():
    global X

    if X > 0:
        del X


# The enclosing function also declares `X` as global, so there's no local `X` to skip.
def f():
    global X

    def g():
        global X

        print(X)


def f():
    global X

    X = 1

    def g():
        global X

        print(X)


def f():
    global X

    del X

    def g():
        global X

        print(X)


# Names bound in a class scope aren't visible to the functions nested within it.
class C:
    X = 1

    def f(self):
        global X

        print(X)


# The enclosing function binds `X`, but nothing reads it.
def f():
    X = 1

    def g():
        global X


# The nearest enclosing binding is a `global`, which makes the outer local unreachable anyway.
def f():
    X = 1

    def g():
        global X

        def h():
            global X

            print(X)


###
# Non-errors.
###
def f():
    global X

    X = 1


def f():
    global X

    (X, y) = (1, 2)


def f():
    global X

    del X


def f():
    global X

    X += 1


# The enclosing function binds `X`, so `global` is needed to read the global `X`.
def f():
    X = 1

    def g():
        global X

        print(X)


def f():
    X = 1

    def g():
        def h():
            global X

            print(X)


def f():
    X = 1

    def g():
        global X

        def h():
            print(X)


# The enclosing function's type parameter `X` would otherwise be read instead of the global `X`.
def f[X]():
    def g():
        global X

        print(X)


# `g` rebinds `f`'s `X` with `nonlocal`, so `global` is needed to read the global `X`.
def f():
    X = 1

    def g():
        nonlocal X

        def h():
            global X

            print(X)
