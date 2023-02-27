def f12(
    x,
    y=os.pathsep,  # OK
) -> None:
    ...


def f11(*, x="x") -> None:
    ...  # OK


def f13(
    x=[  # OK
        "foo",
        "bar",
        "baz",
    ]
) -> None:
    ...


def f14(
    x=(  # OK
        "foo",
        "bar",
        "baz",
    )
) -> None:
    ...


def f15(
    x={  # OK
        "foo",
        "bar",
        "baz",
    }
) -> None:
    ...


def f16(x=frozenset({b"foo", b"bar", b"baz"})) -> None:
    ...  # OK


def f17(
    x="foo" + "bar",  # OK
) -> None:
    ...


def f18(
    x=b"foo" + b"bar",  # OK
) -> None:
    ...


def f19(
    x="foo" + 4,  # OK
) -> None:
    ...


def f20(
    x=5 + 5,  # OK
) -> None:
    ...


def f21(
    x=3j - 3j,  # OK
) -> None:
    ...


def f22(
    x=-42.5j + 4.3j,  # OK
) -> None:
    ...
