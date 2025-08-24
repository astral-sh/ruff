import typing
from typing import Union


def f(x: Union[str, int, Union[float, bytes]]) -> None:
    ...


def f(x: typing.Union[str, int]) -> None:
    ...


def f(x: typing.Union[(str, int)]) -> None:
    ...


def f(x: typing.Union[(str, int), float]) -> None:
    ...


def f(x: typing.Union[(int,)]) -> None:
    ...


def f(x: typing.Union[()]) -> None:
    ...


def f(x: "Union[str, int, Union[float, bytes]]") -> None:
    ...


def f(x: "typing.Union[str, int]") -> None:
    ...


def f(x: Union["str", int]) -> None:
    ...


def f(x: Union[("str", "int"), float]) -> None:
    ...


def f() -> None:
    x = Union[str, int]
    x = Union["str", "int"]
    x: Union[str, int]
    x: Union["str", "int"]


def f(x: Union[int : float]) -> None:
    ...


def f(x: Union[str, int : float]) -> None:
    ...


def f(x: Union[x := int]) -> None:
    ...


def f(x: Union[str, x := int]) -> None:
    ...


def f(x: Union[lambda: int]) -> None:
    ...


def f(x: Union[str, lambda: int]) -> None:
    ...


# Regression test for: https://github.com/astral-sh/ruff/issues/7452
class Collection(Protocol[*_B0]):
    def __iter__(self) -> Iterator[Union[*_B0]]:
        ...


# Regression test for: https://github.com/astral-sh/ruff/issues/8609
def f(x: Union[int, str, bytes]) -> None:
    ...


# Regression test for https://github.com/astral-sh/ruff/issues/14132
class AClass:
    ...

def myfunc(param: "tuple[Union[int, 'AClass', None], str]"):
    print(param)


from typing import NamedTuple, Union

import typing_extensions
from typing_extensions import (
    NamedTuple as NamedTupleTE,
    Union as UnionTE,
)

# Regression test for https://github.com/astral-sh/ruff/issues/18619
# Don't emit lint for `NamedTuple`
a_plain_1: Union[NamedTuple, int] = None
a_plain_2: Union[int, NamedTuple] = None
a_plain_3: Union[NamedTuple, None] = None
a_plain_4: Union[None, NamedTuple] = None
a_plain_te_1: UnionTE[NamedTupleTE, int] = None
a_plain_te_2: UnionTE[int, NamedTupleTE] = None
a_plain_te_3: UnionTE[NamedTupleTE, None] = None
a_plain_te_4: UnionTE[None, NamedTupleTE] = None
a_plain_typing_1: UnionTE[typing.NamedTuple, int] = None
a_plain_typing_2: UnionTE[int, typing.NamedTuple] = None
a_plain_typing_3: UnionTE[typing.NamedTuple, None] = None
a_plain_typing_4: UnionTE[None, typing.NamedTuple] = None
a_string_1: "Union[NamedTuple, int]" = None
a_string_2: "Union[int, NamedTuple]" = None
a_string_3: "Union[NamedTuple, None]" = None
a_string_4: "Union[None, NamedTuple]" = None
a_string_te_1: "UnionTE[NamedTupleTE, int]" = None
a_string_te_2: "UnionTE[int, NamedTupleTE]" = None
a_string_te_3: "UnionTE[NamedTupleTE, None]" = None
a_string_te_4: "UnionTE[None, NamedTupleTE]" = None
a_string_typing_1: "typing.Union[typing.NamedTuple, int]" = None
a_string_typing_2: "typing.Union[int, typing.NamedTuple]" = None
a_string_typing_3: "typing.Union[typing.NamedTuple, None]" = None
a_string_typing_4: "typing.Union[None, typing.NamedTuple]" = None

b_plain_1: Union[NamedTuple] = None
b_plain_2: Union[NamedTuple, None] = None
b_plain_te_1: UnionTE[NamedTupleTE] = None
b_plain_te_2: UnionTE[NamedTupleTE, None] = None
b_plain_typing_1: UnionTE[typing.NamedTuple] = None
b_plain_typing_2: UnionTE[typing.NamedTuple, None] = None
b_string_1: "Union[NamedTuple]" = None
b_string_2: "Union[NamedTuple, None]" = None
b_string_te_1: "UnionTE[NamedTupleTE]" = None
b_string_te_2: "UnionTE[NamedTupleTE, None]" = None
b_string_typing_1: "typing.Union[typing.NamedTuple]" = None
b_string_typing_2: "typing.Union[typing.NamedTuple, None]" = None
