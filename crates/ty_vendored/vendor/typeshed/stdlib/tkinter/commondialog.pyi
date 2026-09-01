from collections.abc import Mapping
from tkinter import Misc
from typing import Any, ClassVar

__all__ = ["Dialog"]

class Dialog:
    command: ClassVar[str | None]
    master: Misc | None
    # Types of options are very dynamic. They depend on the command and are
    # sometimes changed to a different type.
    options: Mapping[str, Any]
    def __init__(self, master: Misc | None = None, **options: Any) -> None: ...
    def show(self, **options: Any) -> Any: ...
