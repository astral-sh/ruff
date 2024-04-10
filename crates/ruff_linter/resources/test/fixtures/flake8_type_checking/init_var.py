"""Test: avoid marking an `InitVar` as typing-only."""

from __future__ import annotations

from dataclasses import FrozenInstanceError, InitVar, dataclass
from pathlib import Path


@dataclass
class C:
    i: int
    j: int = None
    database: InitVar[Path] = None

    err: FrozenInstanceError = None

    def __post_init__(self, database):
        ...
