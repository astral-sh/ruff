from __future__ import annotations

from typing import TypeVar


x: "int" | str  # OK
x: ("int" | str) | "bool"  # OK


def func():
    x: "int" | str  # OK


z: list[str, str | "int"] = []  # OK

type A = Value["int" | str]  # OK

OldS = TypeVar('OldS', int | 'str', str)  # TC010
