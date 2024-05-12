from typing import Optional


def f(x: Optional[str]) -> None:  # noqa: U007B
    ...


def f(x: Optional[str]) -> None:  # noqa: UP007B
    ...
