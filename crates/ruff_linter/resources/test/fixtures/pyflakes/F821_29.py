"""Regression test for #10451.

Annotations in a class are allowed to be forward references
if `from __future__ import annotations` is active,
even if they're in a class included in
`lint.flake8-type-checking.runtime-evaluated-base-classes`.

They're not allowed to refer to symbols that cannot be *resolved*
at runtime, however.
"""

from __future__ import annotations

from sqlalchemy.orm import DeclarativeBase, Mapped


class Base(DeclarativeBase):
    some_mapping: Mapped[list[Bar]] | None = None  # Should not trigger F821 (resolveable forward reference)
    simplified: list[Bar] | None = None  # Should not trigger F821 (resolveable forward reference)


class Bar:
    pass
