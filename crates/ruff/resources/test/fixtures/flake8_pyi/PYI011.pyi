def f12(
    x,
    y: str = os.pathsep,  # Error PYI011 Only simple default values allowed for typed arguments
) -> None: ...
def f11(*, x: str = "x") -> None: ...  # OK
def f13(
    x: list[
        str
    ] = [  # Error PYI011 Only simple default values allowed for typed arguments
        "foo",
        "bar",
        "baz",
    ]
) -> None: ...
def f14(
    x: tuple[
        str, ...
    ] = (  # Error PYI011 Only simple default values allowed for typed arguments
        "foo",
        "bar",
        "baz",
    )
) -> None: ...
def f15(
    x: set[
        str
    ] = {  # Error PYI011 Only simple default values allowed for typed arguments
        "foo",
        "bar",
        "baz",
    }
) -> None: ...
def f16(
    x: frozenset[
        bytes
    ] = frozenset(  # Error PYI011 Only simple default values allowed for typed arguments
        {b"foo", b"bar", b"baz"}
    )
) -> None: ...
def f17(
    x: str = "foo"  # Error PYI011 Only simple default values allowed for typed arguments
    + "bar",
) -> None: ...
def f18(
    x: str = b"foo"  # Error PYI011 Only simple default values allowed for typed arguments
    + b"bar",
) -> None: ...
def f19(
    x: object = "foo"  # Error PYI011 Only simple default values allowed for typed arguments
    + 4,
) -> None: ...
def f20(
    x: int = 5
    + 5,  # Error PYI011 Only simple default values allowed for typed arguments
) -> None: ...
def f21(
    x: complex = 3j
    - 3j,  # Error PYI011 Only simple default values allowed for typed arguments
) -> None: ...
def f22(
    x: complex = -42.5j  # Error PYI011 Only simple default values allowed for typed arguments
    + 4.3j,
) -> None: ...
