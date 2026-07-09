from __future__ import annotations

from ._find_ruff import find_ruff_bin

try:
    from ._ruff_api import format
except ImportError:
    # Native module not available (e.g., binary-only wheel install).
    # The format() function requires building with PyO3 support.
    pass

__all__ = ["find_ruff_bin", "format"]
