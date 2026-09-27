"""Test that `# ruff: ignore` protects an unused import member from the shared fix.

Regression test for https://github.com/astral-sh/ruff/issues/26282 (item 1):
the combined unused-import fix should not remove a member that is suppressed by
a `# ruff: ignore` comment, mirroring the existing `# noqa` behavior.
"""

from package import (
    kept,  # noqa: F401
    removed,
)

from package2 import (
    kept2,  # ruff: ignore[F401]
    removed2,
)
