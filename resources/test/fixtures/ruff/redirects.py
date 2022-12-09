from typing import Optional


def f(x: Optional[str]) -> None:  # noqa: U007
    ...


def f(x: Optional[str]) -> None:  # noqa: UP007
    ...
