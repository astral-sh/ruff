"""Tests that B008 recognizes frozen dataclass instantiations with immutable fields."""

from dataclasses import dataclass
from typing import ClassVar


@dataclass(frozen=True)
class Foo:
    val: bool = True
    count: int = 0
    name: str = "foo"


def bar(foo: Foo = Foo()) -> None:
    ...


@dataclass(frozen=True)
class FrozenMutableField:
    items: list = []


def baz(x: FrozenMutableField = FrozenMutableField()) -> None:
    ...


@dataclass
class NotFrozen:
    val: bool = True


def qux(x: NotFrozen = NotFrozen()) -> None:
    ...


@dataclass(frozen=True)
class WithClassVar:
    val: bool = True
    counter: ClassVar[list[int]] = []


def with_class_var(x: WithClassVar = WithClassVar()) -> None:
    ...


@dataclass(frozen=True)
class WithTupleField:
    items: tuple[int, ...] = (1, 2, 3)


def with_tuple(x: WithTupleField = WithTupleField()) -> None:
    ...


@dataclass(frozen=True)
class WithMutableDefault:
    val: int = []


def with_mutable_default(x: WithMutableDefault = WithMutableDefault()) -> None:
    ...
