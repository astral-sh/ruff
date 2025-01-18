from __future__ import annotations

from dataclasses import dataclass


class Point:  # B903
    def __init__(self, x: float, y: float) -> None:
        self.x = x
        self.y = y


class Rectangle:  # OK
    def __init__(self, top_left: Point, bottom_right: Point) -> None:
        ...

    def area(self) -> float:
        ...


@dataclass
class Circle:  # OK
    center: Point
    radius: float

    def area(self) -> float:
        ...


class CustomException(Exception):  # OK
    ...


class A:   # OK
    class B:
        ...
    
    def __init__(self):
        ...

class C:   # B903
    c: int
    def __init__(self,d:list):
        self.d = d

class D:   # B903
    """This class has a docstring."""
    # this next method is an init
    def __init__(self,e:dict):
        self.e = e

# <--- begin flake8-bugbear tests below
# (we have modified them to have type annotations,
# since our implementation only triggers in that
# stricter setting.)
class NoWarningsMoreMethods:
    def __init__(self, foo:int, bar:list):
        self.foo = foo
        self.bar = bar

    def other_function(self): ...


class NoWarningsClassAttributes:
    spam = "ham"

    def __init__(self, foo:int, bar:list):
        self.foo = foo
        self.bar = bar


class NoWarningsComplicatedAssignment:
    def __init__(self, foo:int, bar:list):
        self.foo = foo
        self.bar = bar
        self.spam = " - ".join([foo, bar])


class NoWarningsMoreStatements:
    def __init__(self, foo:int, bar:list):
        foo = " - ".join([foo, bar])
        self.foo = foo
        self.bar = bar


class Warnings:
    def __init__(self, foo:int, bar:list):
        self.foo = foo
        self.bar = bar


class WarningsWithDocstring:
    """A docstring should not be an impediment to a warning"""

    def __init__(self, foo:int, bar:list):
        self.foo = foo
        self.bar = bar


class KeywordOnly: # OK with python3.9 or less, not OK starting python3.10
    def __init__(self, *, foo: int, bar: int):
        self.foo = foo
        self.bar = bar 

# <-- end flake8-bugbear tests
