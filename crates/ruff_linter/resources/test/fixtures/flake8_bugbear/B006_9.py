LIST = []


def foo(a: list = LIST):  # B006
    raise NotImplementedError("")


def foo(a: list = None):  # OK
    raise NotImplementedError("")


def foo(a: list | None = LIST):  # OK
    raise NotImplementedError("")
