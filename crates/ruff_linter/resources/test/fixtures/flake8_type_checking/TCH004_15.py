from __future__ import annotations

from collections.abc import Callable
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .foo import Record

type RecordOrThings = Record | int | str
type RecordCallback[R: Record] = Callable[[R], None]


def process_record[R: Record](record: R) -> None:
    ...


class RecordContainer[R: Record]:
    def add_record(self, record: R) -> None:
        ...
