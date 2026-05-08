"""
Regression test for https://github.com/astral-sh/ruff/issues/20782.

On Python 3.14+, annotations are deferred (PEP 649), so `UP037` still considers
these quotes unnecessary. But tools that introspect annotations eagerly --
`inspect.signature(eval_str=True)`, `unittest.mock.create_autospec`, etc. --
can still raise `NameError` for names that are only imported under
`TYPE_CHECKING`. Removing the quotes here is therefore an unsafe fix unless
the file already opts in to `from __future__ import annotations`.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Tuple


def foo(x: "Tuple[int, ...]") -> "Tuple[int, ...]":
    return x


def bar() -> None:
    y: "Tuple[int, ...]" = (0, 0)
    print(y)
