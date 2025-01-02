"""
Regression test that makes sure we do not short-circuit here after
determining that the overall type will be `Never` and still infer
a type for the second tuple element `2`.

Relevant discussion:
https://github.com/astral-sh/ruff/pull/15218#discussion_r1900811073
"""

from typing_extensions import Never


def never() -> Never:
    return never()


(never(), 2)
