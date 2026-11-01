from tkinter import Misc
from tkinter.commondialog import Dialog
from typing import ClassVar

__all__ = ["Chooser", "askcolor"]

class Chooser(Dialog):
    command: ClassVar[str]

def askcolor(
    color: str | bytes | None = None, *, initialcolor: str = ..., parent: Misc = ..., title: str = ...
) -> tuple[None, None] | tuple[tuple[int, int, int], str]: ...
