from __future__ import annotations

from typing import Annotated


# OK
def okay_fn(
    a: int, b: str, c: list[str, int, 0], d: Annotated[list[str], "yes"]
) -> list[int]: ...


# Error
def complex_argument(
    a: list[dict[str, list[int]]], d: Annotated[dict[str, list[list[int]]], "yes"]
) -> list[int]: ...


# Error
def complex_return(a: int) -> dict[str, list[dict[str, int]]]: ...
