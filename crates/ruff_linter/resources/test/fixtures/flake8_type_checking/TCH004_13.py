from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Final, Literal, TypeAlias

    RatingKey: TypeAlias = Literal["good", "fair", "poor"]

RATING_KEYS: Final[tuple[RatingKey, ...]] = ("good", "fair", "poor")
