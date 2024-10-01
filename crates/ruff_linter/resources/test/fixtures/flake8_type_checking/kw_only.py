"""Test: avoid marking a `KW_ONLY` annotation as typing-only."""

from __future__ import annotations

from dataclasses import KW_ONLY, dataclass, Field


@dataclass
class Test1:
    a: int
    _: KW_ONLY
    b: str


@dataclass
class Test2:
    a: int
    b: Field
