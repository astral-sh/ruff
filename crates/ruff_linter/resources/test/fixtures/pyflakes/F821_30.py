"""Regression test for #13824.

Don't report an error when the function being annotated has the
`@no_type_check` decorator.
"""

from typing import no_type_check


@no_type_check
def f(arg: "A") -> "R":
    x: "A" = 1


@no_type_check
class C:
    def f(self, arg: "B") -> "S":
        x: "B" = 1
