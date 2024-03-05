from __future__ import annotations

from typing import TypeVar


x: "int" | str  # TCH010
x: ("int" | str) | "bool"  # TCH010


def func():
    x: "int" | str  # OK


z: list[str, str | "int"] = []  # TCH010

type A = Value["int" | str]  # OK

OldS = TypeVar('OldS', int | 'str', str)  # TCH010
