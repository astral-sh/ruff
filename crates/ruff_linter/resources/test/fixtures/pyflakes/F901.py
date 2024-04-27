def f() -> None:
    raise NotImplemented()


def g() -> None:
    raise NotImplemented


def h() -> None:
    NotImplementedError = "foo"
    raise NotImplemented
