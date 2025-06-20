from __future__ import annotations


# https://github.com/astral-sh/ruff/issues/18298
# fix must not yield runtime `None | None | ...` (TypeError)
class Issue18298:
    def f1(self, arg: None | int | None | float = None) -> None:
        pass
