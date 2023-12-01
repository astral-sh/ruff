import math
import os
import sys
from math import inf

import numpy as np

def f12(
    x,
    y: str = os.pathsep,  # OK
) -> None: ...
def f11(*, x: str = "x") -> None: ...  # OK
def f13(
    x: list[str] = [  # OK
        "foo",
        "bar",
        "baz",
    ]
) -> None: ...
def f14(
    x: tuple[str, ...] = (  # OK
        "foo",
        "bar",
        "baz",
    )
) -> None: ...
def f15(
    x: set[str] = {  # OK
        "foo",
        "bar",
        "baz",
    }
) -> None: ...
def f151(x: dict[int, int] = {1: 2}) -> None: ...  # Ok
def f152(
    x: dict[
        int, int
    ] = {  # OK
        1: 2,
        **{3: 4},
    }
) -> None: ...
def f153(
    x: list[
        int
    ] = [  # OK
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
) -> None: ...
def f154(
    x: tuple[
        str, tuple[str, ...]
    ] = (  # OK
        "foo",
        ("bar", "baz"),
    )
) -> None: ...
def f141(
    x: list[
        int
    ] = [  # OK
        *range(10)
    ],
) -> None: ...
def f142(
    x: list[
        int
    ] = list(  # OK
        range(10)
    ),
) -> None: ...
def f16(
    x: frozenset[
        bytes
    ] = frozenset(  # OK
        {b"foo", b"bar", b"baz"}
    )
) -> None: ...
def f17(
    x: str = "foo"  # OK
    + "bar",
) -> None: ...
def f18(
    x: str = b"foo"  # OK
    + b"bar",
) -> None: ...
def f19(
    x: object = "foo"  # OK
    + 4,
) -> None: ...
def f20(
    x: int = 5
    + 5,  # OK
) -> None: ...
def f21(
    x: complex = 3j
    - 3j,  # OK
) -> None: ...
def f22(
    x: complex = -42.5j  # OK
    + 4.3j,
) -> None: ...
def f23(
    x: bool = True,  # OK
) -> None: ...
def f24(
    x: float = 3.14,  # OK
) -> None: ...
def f25(
    x: float = -3.14,  # OK
) -> None: ...
def f26(
    x: complex = -3.14j,  # OK
) -> None: ...
def f27(
    x: complex = -3 - 3.14j,  # OK
) -> None: ...
def f28(
    x: float = math.tau,  # OK
) -> None: ...
def f29(
    x: float = math.inf,  # OK
) -> None: ...
def f30(
    x: float = -math.inf,  # OK
) -> None: ...
def f31(
    x: float = inf,  # OK
) -> None: ...
def f32(
    x: float = np.inf,  # OK
) -> None: ...
def f33(
    x: float = math.nan,  # OK
) -> None: ...
def f34(
    x: float = -math.nan,  # OK
) -> None: ...
def f35(
    x: complex = math.inf  # OK
    + 1j,
) -> None: ...
def f36(
    *,
    x: str = sys.version,  # OK
) -> None: ...
def f37(
    *,
    x: str = ""  # OK
    + "",
) -> None: ...
