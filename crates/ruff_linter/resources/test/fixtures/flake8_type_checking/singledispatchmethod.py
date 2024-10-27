from __future__ import annotations

from collections.abc import MutableMapping, Mapping
from pathlib import Path
from functools import singledispatchmethod
from typing import Union


class Foo:
    @singledispatchmethod
    def foo(self, x: Union[MutableMapping, Mapping]) -> int:
        raise NotImplementedError

    @foo.register
    def _(self, x: MutableMapping) -> int:
        return 0

    @foo.register
    def _(self, x: Mapping) -> int:
        return 0


class Foo2:

    @singledispatchmethod
    def process_path(self, a: Union[int, str]) -> int:
        """Convert arg to array or leaves it as sparse matrix."""
        msg = f"Unhandled type {type(a)}"
        raise NotImplementedError(msg)


    @process_path.register
    def _(self, a: int) -> int:
        return a


    @process_path.register
    def _(self, a: str) -> int:
        return len(a)


    def _(self, p: Path) -> Path:
        return p