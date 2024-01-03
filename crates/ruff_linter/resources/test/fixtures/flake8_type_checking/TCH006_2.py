from typing import TypeVar


x: "int" | str  # TCH006
x: ("int" | str) | "bool"  # TCH006


def func():
    x: "int" | str  # OK


z: list[str, str | "int"] = []  # TCH006

type A = Value["int" | str]  # OK

OldS = TypeVar('OldS', int | 'str', str)  # TCH006
