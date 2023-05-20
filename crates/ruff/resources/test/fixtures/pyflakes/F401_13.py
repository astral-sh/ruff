"""Test: module bindings are preferred over local bindings, for deferred annotations."""

from __future__ import annotations

from typing import TypeAlias, List


class Class:
    List: TypeAlias = List

    def bar(self) -> List:
        pass
