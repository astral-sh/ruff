# Protocol interface recovery

This is a reduced regression test from Antidote. A protocol method annotation can mention a
sub-protocol of the protocol whose interface is currently being recovered.

```py
from __future__ import annotations

from typing import Callable, Protocol


class Catalog(Protocol):
    def include(self, obj: Callable[[Catalog], object] | PublicCatalog) -> None: ...


class PublicCatalog(Catalog, Protocol):
    @property
    def test(self) -> object: ...


class CatalogImpl:
    test: object

    @classmethod
    def create_public(cls) -> PublicCatalog:
        return cls()

    def include(self, obj: Callable[[Catalog], object] | PublicCatalog) -> None: ...


world: PublicCatalog = CatalogImpl.create_public()
```
