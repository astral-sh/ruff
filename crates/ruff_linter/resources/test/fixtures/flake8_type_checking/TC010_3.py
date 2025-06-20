from typing import TypeVar, TYPE_CHECKING

import foo

if TYPE_CHECKING:
    from foo import Foo


x: "Foo" | str  # TC010
x: ("int" | str) | "Foo"  # TC010


def func():
    x: "Foo" | str  # OK


z: list[str, str | "list[str]"] = []  # TC010

type A = Value["Foo" | str]  # OK

OldS = TypeVar('OldS', int | 'foo.Bar', str)  # TC010

x: ("Foo"  # TC010 (unsafe fix)
    | str)
