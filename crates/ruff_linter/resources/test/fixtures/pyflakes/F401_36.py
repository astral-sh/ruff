"""Test: ensure `ruff:ignore` protects imports from being removed by autofixes,
the same way `noqa` does. See: https://github.com/astral-sh/ruff/issues/26282"""

from package import (
    kept,  # noqa: F401
    removed,
)

from package2 import (
    kept2,  # ruff:ignore[F401]
    removed2,
)
