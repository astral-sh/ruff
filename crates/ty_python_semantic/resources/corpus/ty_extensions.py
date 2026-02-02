"""
Make sure that types are inferred for all subexpressions of the following
annotations involving ty_extension `_SpecialForm`s.

This is a regression test for https://github.com/astral-sh/ty/issues/366
"""

from ty_extensions import CallableTypeOf, Intersection, Not, TypeOf


class A: ...


class B: ...


def _(x: Not[A]):
    pass


def _(x: Intersection[A], y: Intersection[A, B]):
    pass


def _(x: TypeOf[1j]):
    pass


def _(x: CallableTypeOf[str]):
    pass
