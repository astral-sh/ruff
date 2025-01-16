"""Regression test for https://github.com/astral-sh/ruff/issues/12897"""

__all__ = ('Spam',)


class Spam:
    def __init__(self) -> None:
        from F401_33.other import Ham
