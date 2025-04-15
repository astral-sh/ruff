from _typeshed import Incomplete
from collections.abc import Mapping
from tkinter import Widget
from typing import Any, Final

__all__ = ["Dialog"]

DIALOG_ICON: Final = "questhead"

class Dialog(Widget):
    widgetName: str
    num: int
    def __init__(self, master: Incomplete | None = None, cnf: Mapping[str, Any] = {}, **kw) -> None: ...
    def destroy(self) -> None: ...
