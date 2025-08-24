def func(): ...


def func():
    pass


def func():
    x = 1
    x = 2


def func():
    foo()


def func():
    from foo import bar

    class C:
        a = 1

    c = C()
    del c
