from tkinter import Misc
from tkinter.commondialog import Dialog
from typing import ClassVar

__all__ = ["Chooser", "askcolor"]

class Chooser(Dialog):
    """Create a dialog for the tk_chooseColor command.

    Args:
        master: The master widget for this dialog.  If not provided,
            defaults to options['parent'] (if defined).
        options: Dictionary of options for the tk_chooseColor call.
            initialcolor: Specifies the selected color when the
                dialog is first displayed.  This can be a tk color
                string or a 3-tuple of ints in the range (0, 255)
                for an RGB triplet.
            parent: The parent window of the color dialog.  The
                color dialog is displayed on top of this.
            title: A string for the title of the dialog box.
    """

    command: ClassVar[str]

def askcolor(
    color: str | bytes | None = None, *, initialcolor: str = ..., parent: Misc = ..., title: str = ...
) -> tuple[None, None] | tuple[tuple[int, int, int], str]:
    """Display dialog window for selection of a color.

    Convenience wrapper for the Chooser class.  Displays the color
    chooser dialog with color as the initial value.
    """
