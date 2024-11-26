"""Regression test for #13824.

Don't report an error when the function being annotated has the
`@no_type_check` decorator.
"""

from typing import no_type_check


@no_type_check
def f(arg: "this isn't python") -> "this isn't python either":
    pass
