from __future__ import annotations


class Point:  # PLR0903
    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y


class Rectangle:  # OK
    def __init__(self, top_left: Point, bottom_right: Point) -> None:
        ...

    def area(self) -> float:
        ...
