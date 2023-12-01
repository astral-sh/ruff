"""Regression test: ensure that we don't treat the export entry as a typing-only reference."""
from __future__ import annotations

from logging import getLogger

__all__ = ("getLogger",)


def foo() -> None:
    pass
