"""Test that type parameters are considered used."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Callable

    from .foo import Record as Record1
    from .bar import Record as Record2

type RecordCallback[R: Record1] = Callable[[R], None]


def process_record[R: Record2](record: R) -> None:
    ...
