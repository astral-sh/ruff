def f12(
    x,
    y=os.pathsep,  # Error PYI014
) -> None: ...
def f11(*, x="x") -> None: ...  # OK
def f13(
    x=[  # Error PYI014
        "foo",
        "bar",
        "baz",
    ]
) -> None: ...
def f14(
    x=(  # Error PYI014
        "foo",
        "bar",
        "baz",
    )
) -> None: ...
def f15(
    x={  # Error PYI014
        "foo",
        "bar",
        "baz",
    }
) -> None: ...
def f16(x=frozenset({b"foo", b"bar", b"baz"})) -> None: ...  # Error PYI014
def f17(
    x="foo" + "bar",  # Error PYI014
) -> None: ...
def f18(
    x=b"foo" + b"bar",  # Error PYI014
) -> None: ...
def f19(
    x="foo" + 4,  # Error PYI014
) -> None: ...
def f20(
    x=5 + 5,  # Error PYI014
) -> None: ...
def f21(
    x=3j - 3j,  # Error PYI014
) -> None: ...
def f22(
    x=-42.5j + 4.3j,  # Error PYI014
) -> None: ...
def f23(
  x=True,  # OK
) -> None: ...
