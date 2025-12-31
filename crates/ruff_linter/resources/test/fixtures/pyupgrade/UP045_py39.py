"""
Regression test for https://github.com/astral-sh/ruff/issues/20096
"""

from __future__ import annotations

from typing import Optional, cast


x: Optional[str]             # UP045
x: "Optional[str]"           # UP045
cast("Optional[str]", None)  # UP045
cast(Optional[str], None)    # okay, str | None is a runtime error
