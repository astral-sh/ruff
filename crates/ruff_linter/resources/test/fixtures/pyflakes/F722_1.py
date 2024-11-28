"""Regression test for #13824.

Don't report an error when the function being annotated has the
`@no_type_check` decorator.

However, we still want to ignore this annotation on classes. See
https://github.com/python/typing/pull/1615/files and the discussion on #14615.
"""

from typing import no_type_check


@no_type_check
def f(arg: "this isn't python") -> "this isn't python either":
    x: "this also isn't python" = 0


@no_type_check
class C:
    def f(arg: "this isn't python") -> "this isn't python either":
        x: "this also isn't python" = 1
