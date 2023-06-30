from __future__ import annotations

import typing

if typing.TYPE_CHECKING:
    from collections import defaultdict


def f(x: typing.DefaultDict[str, str]) -> None:
    ...


from collections.abc import Set
from typing_extensions import Awaitable


def f(x: typing.AbstractSet[str]) -> None:
    ...


def f(x: Set) -> None:
    ...


def f(x: Awaitable) -> None:
    ...
