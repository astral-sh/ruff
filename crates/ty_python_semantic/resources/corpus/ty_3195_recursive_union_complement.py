# Regression test for https://github.com/astral-sh/ty/issues/3195.

from typing import reveal_type
from ty_extensions import Not

type A = list[Not[A] | A]

def f(x: A):
    reveal_type(x[])
