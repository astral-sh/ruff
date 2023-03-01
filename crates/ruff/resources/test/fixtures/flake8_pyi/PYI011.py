def f12(
    x,
    y: str = os.pathsep,  # OK
) -> None:
    ...


def f11(*, x: str = "x") -> None:  # OK
    ...


def f13(
    x: list[str] = [
        "foo",
        "bar",
        "baz",
    ]  # OK
) -> None:
    ...


def f14(
    x: tuple[str, ...] = (
        "foo",
        "bar",
        "baz",
    )  # OK
) -> None:
    ...


def f15(
    x: set[str] = {
        "foo",
        "bar",
        "baz",
    }  # OK
) -> None:
    ...


def f16(x: frozenset[bytes] = frozenset({b"foo", b"bar", b"baz"})) -> None:  # OK
    ...


def f17(
    x: str = "foo" + "bar",  # OK
) -> None:
    ...


def f18(
    x: str = b"foo" + b"bar",  # OK
) -> None:
    ...


def f19(
    x: object = "foo" + 4,  # OK
) -> None:
    ...


def f20(
    x: int = 5 + 5,  # OK
) -> None:
    ...


def f21(
    x: complex = 3j - 3j,  # OK
) -> None:
    ...


def f22(
    x: complex = -42.5j + 4.3j,  # OK
) -> None:
    ...
