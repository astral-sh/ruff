"""This modules handles dialog boxes.

It contains the following public symbols:

SimpleDialog -- A simple but flexible modal dialog box

Dialog -- a base class for dialogs

askinteger -- get an integer from the user

askfloat -- get a float from the user

askstring -- get a string from the user
"""

from tkinter import Event, Frame, Misc, Toplevel

class Dialog(Toplevel):
    """Class to open dialogs.

    This class is intended as a base class for custom dialogs
    """

    def __init__(self, parent: Misc | None, title: str | None = None) -> None:
        """Initialize a dialog.

        Arguments:

            parent -- a parent window (the application window)

            title -- the dialog title
        """

    def body(self, master: Frame) -> Misc | None:
        """create dialog body.

        return widget that should have initial focus.
        This method should be overridden, and is called
        by the __init__ method.
        """

    def buttonbox(self) -> None:
        """add standard button box.

        override if you do not want the standard buttons
        """

    def ok(self, event: Event[Misc] | None = None) -> None: ...
    def cancel(self, event: Event[Misc] | None = None) -> None: ...
    def validate(self) -> bool:
        """validate the data

        This method is called automatically to validate the data before the
        dialog is destroyed. By default, it always validates OK.
        """

    def apply(self) -> None:
        """process the data

        This method is called automatically to process the data, *after*
        the dialog is destroyed. By default, it does nothing.
        """

class SimpleDialog:
    def __init__(
        self,
        master: Misc | None,
        text: str = "",
        buttons: list[str] = [],
        default: int | None = None,
        cancel: int | None = None,
        title: str | None = None,
        class_: str | None = None,
    ) -> None: ...
    def go(self) -> int | None: ...
    def return_event(self, event: Event[Misc]) -> None: ...
    def wm_delete_window(self) -> None: ...
    def done(self, num: int) -> None: ...

def askfloat(
    title: str | None,
    prompt: str,
    *,
    initialvalue: float | None = ...,
    minvalue: float | None = ...,
    maxvalue: float | None = ...,
    parent: Misc | None = ...,
) -> float | None:
    """get a float from the user

    Arguments:

        title -- the dialog title
        prompt -- the label text
        **kw -- see SimpleDialog class

    Return value is a float
    """

def askinteger(
    title: str | None,
    prompt: str,
    *,
    initialvalue: int | None = ...,
    minvalue: int | None = ...,
    maxvalue: int | None = ...,
    parent: Misc | None = ...,
) -> int | None:
    """get an integer from the user

    Arguments:

        title -- the dialog title
        prompt -- the label text
        **kw -- see SimpleDialog class

    Return value is an integer
    """

def askstring(
    title: str | None,
    prompt: str,
    *,
    initialvalue: str | None = ...,
    show: str | None = ...,
    # minvalue/maxvalue is accepted but not useful.
    parent: Misc | None = ...,
) -> str | None:
    """get a string from the user

    Arguments:

        title -- the dialog title
        prompt -- the label text
        **kw -- see SimpleDialog class

    Return value is a string
    """
