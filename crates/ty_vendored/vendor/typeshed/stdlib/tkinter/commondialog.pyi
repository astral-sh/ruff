from _typeshed import Incomplete
from collections.abc import Mapping
from typing import ClassVar

__all__ = ["Dialog"]

class Dialog:
    command: ClassVar[str | None]
    master: Incomplete | None
    options: Mapping[str, Incomplete]
    def __init__(self, master=None, **options) -> None: ...
    def show(self, **options): ...
