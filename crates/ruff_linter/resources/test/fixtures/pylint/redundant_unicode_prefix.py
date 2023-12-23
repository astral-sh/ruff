def u_string() -> None:
    print(u"Hello, world!")  # PLW1406
    print(U"Hello, world!")  # PLW1406


def b_string() -> None:
    print(b"Hello, world!")  # OK


def r_string() -> None:
    print(r"Hello, world!")  # OK


def f_string() -> None:
    print(f"Hello, world!")  # OK


def string() -> None:
    print("Hello, world!")  # OK
    print("u")  # OK
