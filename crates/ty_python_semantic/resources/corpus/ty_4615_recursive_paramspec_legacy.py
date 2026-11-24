# Regression test for https://github.com/astral-sh/ty/issues/4615

from typing import Generic, ParamSpec

P = ParamSpec("P")

@0
class C(Generic[P]):
    pass

while -():
    (value := C[value])
