from typing import Generic, TypeVarTuple, Unpack

Shape = TypeVarTuple("Shape")


class C(Generic[Unpack[Shape]]):
    pass


class D(Generic[Unpack[Shape]]):
    pass


def f(*args: Unpack[tuple[int, ...]]):
    pass


def f(*args: Unpack[other.Type]):
    pass


def f(*args: Generic[int, Unpack[int]]):
    pass


# Valid syntax, but can't be unpacked.
def f(*args: Unpack[int | str]) -> None:
    pass


def f(*args: Unpack[int and str]) -> None:
    pass


def f(*args: Unpack[int > str]) -> None:
    pass


from typing import TypedDict


class KwargsDict(TypedDict):
    x: int
    y: int


# OK
def f(name: str, /, **kwargs: Unpack[KwargsDict]) -> None:
    pass


# OK
def f() -> object:
    return Unpack[tuple[int, ...]]


# OK
def f(x: Unpack[int]) -> object: ...
