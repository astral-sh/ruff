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


def f151(x={1: 2}) -> None:  # Ok
    ...


def f152(
    x={  # OK
        1: 2,
        **{3: 4},
    }
) -> None:
    ...


def f153(
    x=[  # OK
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
    ]
) -> None:
    ...


def f154(
    x=(  # OK
        "foo",
        ("bar", "baz"),
    )
) -> None:
    ...


def f141(
    x=[*range(10)],  # OK
) -> None:
    ...


def f142(
    x=list(range(10)),  # OK
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
