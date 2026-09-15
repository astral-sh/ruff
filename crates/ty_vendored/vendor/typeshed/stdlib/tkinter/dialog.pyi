"""Classic Tk dialog box, wrapping the tk_dialog script."""

from collections.abc import Mapping
from tkinter import Widget
from typing import Any, Final

__all__ = ["Dialog"]

DIALOG_ICON: Final = "questhead"

class Dialog(Widget):
    """A modal dialog box built from the classic (non-themed) Tk widgets."""

    widgetName: str
    num: int
    def __init__(self, master=None, cnf: Mapping[str, Any] = {}, **kw) -> None: ...
    def destroy(self) -> None:
        """Do nothing; the dialog window is already destroyed."""
