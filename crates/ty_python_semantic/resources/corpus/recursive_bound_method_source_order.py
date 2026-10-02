# Regression test for the steam.py ecosystem failure in
# https://github.com/astral-sh/ruff/pull/27176.

from __future__ import annotations

from typing import Protocol, TypeVar


class PartialApp:
    pass


AppT = TypeVar("AppT", bound=PartialApp, covariant=True)


class BaseOwnedBadge(Protocol[AppT]):
    app: AppT

    def __init__(self, app: AppT) -> None:
        pass

    async def progress(self: BaseOwnedBadge[PartialApp]) -> None:
        pass


class FavouriteBadge(BaseOwnedBadge[AppT]):
    def __init__(self, app: AppT) -> None:
        super().__init__(app)
