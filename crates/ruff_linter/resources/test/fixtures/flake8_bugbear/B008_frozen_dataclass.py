# https://github.com/astral-sh/ruff/issues/29071
import dataclasses
from dataclasses import dataclass

import attr
import attrs


@dataclass(frozen=True)
class Frozen:
    val: bool = True


@dataclasses.dataclass(frozen=True)
class QualifiedFrozen:
    val: int = 0


@attrs.frozen
class AttrsFrozen:
    val: int = 0


@attr.frozen
class AttrFrozen:
    val: int = 0


@dataclass
class NotFrozen:
    val: bool = True


@dataclass(frozen=False)
class ExplicitlyNotFrozen:
    val: bool = True


class NotADataclass:
    pass


# OK
def f(foo: Frozen = Frozen()) -> None: ...


def f(foo=Frozen(val=False)): ...


def f(foo=QualifiedFrozen()): ...


def f(foo=AttrsFrozen()): ...


def f(foo=AttrFrozen()): ...


# The frozen check is shallow, matching `RUF009`.
@dataclass(frozen=True)
class FrozenWithMutableField:
    items: list = dataclasses.field(default_factory=list)


def f(foo=FrozenWithMutableField()): ...


# Default values are resolved in the enclosing scope, not the function scope.
class Outer:
    @dataclass(frozen=True)
    class Inner:
        val: int = 0

    def method(self, foo=Inner()): ...


def f(Frozen=Frozen()): ...


def outer():
    @dataclass(frozen=True)
    class Local:
        val: int = 0

    def inner(foo=Local()): ...


# Errors
def f(foo=NotFrozen()): ...


def f(foo=ExplicitlyNotFrozen()): ...


def f(foo=NotADataclass()): ...


# Calls nested inside a frozen dataclass instantiation are still checked.
def f(foo=Frozen(val=bool(len([])))): ...
