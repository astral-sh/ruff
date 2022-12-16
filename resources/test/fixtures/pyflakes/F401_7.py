"""Test: noqa directives."""

from module import (
    A,  # noqa: F401
    B,
)

from module import (
    A,  # noqa: F401
    B,  # noqa: F401
)

from module import (
    A,
    B,
)
