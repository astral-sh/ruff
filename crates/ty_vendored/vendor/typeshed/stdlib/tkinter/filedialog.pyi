"""File selection dialog classes.

Classes:

- FileDialog
- LoadFileDialog
- SaveFileDialog

This module also presents tk common file dialogues, it provides interfaces
to the native file dialogues available in Tk 4.2 and newer, and the
directory dialogue available in Tk 8.3 and newer.
These interfaces were written by Fredrik Lundh, May 1997.
"""

from _typeshed import Incomplete, StrOrBytesPath, StrPath
from collections.abc import Hashable, Iterable
from tkinter import Button, Entry, Event, Frame, Listbox, Misc, Scrollbar, StringVar, Toplevel, commondialog
from typing import IO, ClassVar, Literal

__all__ = [
    "FileDialog",
    "LoadFileDialog",
    "SaveFileDialog",
    "Open",
    "SaveAs",
    "Directory",
    "askopenfilename",
    "asksaveasfilename",
    "askopenfilenames",
    "askopenfile",
    "askopenfiles",
    "asksaveasfile",
    "askdirectory",
]

dialogstates: dict[Hashable, tuple[str, str]]

class FileDialog:
    """Standard file selection dialog -- no checks on selected file.

    Usage:

        d = FileDialog(master)
        fname = d.go(dir_or_file, pattern, default, key)
        if fname is None: ...canceled...
        else: ...open file...

    All arguments to go() are optional.

    The 'key' argument specifies a key in the global dictionary
    'dialogstates', which keeps track of the values for the directory
    and pattern arguments, overriding the values passed in (it does
    not keep track of the default argument!).  If no key is specified,
    the dialog keeps no memory of previous state.  Note that memory is
    kept even when the dialog is canceled.  (All this emulates the
    behavior of the Macintosh file selection dialogs.)

    """

    title: str
    master: Misc
    directory: str | None
    top: Toplevel
    botframe: Frame
    selection: Entry
    filter: Entry
    midframe: Entry
    filesbar: Scrollbar
    files: Listbox
    dirsbar: Scrollbar
    dirs: Listbox
    ok_button: Button
    filter_button: Button
    cancel_button: Button
    def __init__(
        self, master: Misc, title: str | None = None
    ) -> None: ...  # title is usually a str or None, but e.g. int doesn't raise en exception either
    how: str | None
    def go(self, dir_or_file: StrPath = ".", pattern: StrPath = "*", default: StrPath = "", key: Hashable | None = None): ...
    def quit(self, how: str | None = None) -> None: ...
    def dirs_double_event(self, event: Event) -> None: ...
    def dirs_select_event(self, event: Event) -> None: ...
    def files_double_event(self, event: Event) -> None: ...
    def files_select_event(self, event: Event) -> None: ...
    def ok_event(self, event: Event) -> None: ...
    def ok_command(self) -> None: ...
    def filter_command(self, event: Event | None = None) -> None: ...
    def get_filter(self) -> tuple[str, str]: ...
    def get_selection(self) -> str: ...
    def cancel_command(self, event: Event | None = None) -> None: ...
    def set_filter(self, dir: StrPath, pat: StrPath) -> None: ...
    def set_selection(self, file: StrPath) -> None: ...

class LoadFileDialog(FileDialog):
    """File selection dialog which checks that the file exists."""

    title: str
    def ok_command(self) -> None: ...

class SaveFileDialog(FileDialog):
    """File selection dialog which checks that the file may be created."""

    title: str
    def ok_command(self) -> None: ...

class _Dialog(commondialog.Dialog): ...

class Open(_Dialog):
    """Ask for a filename to open"""

    command: ClassVar[str]

class SaveAs(_Dialog):
    """Ask for a filename to save as"""

    command: ClassVar[str]

class Directory(commondialog.Dialog):
    """Ask for a directory"""

    command: ClassVar[str]

# TODO: command kwarg available on macos
def asksaveasfilename(
    *,
    confirmoverwrite: bool | None = True,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> str:  # can be empty string
    """Ask for a filename to save as"""

def askopenfilename(
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> str:  # can be empty string
    """Ask for a filename to open"""

def askopenfilenames(
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> Literal[""] | tuple[str, ...]:
    """Ask for multiple filenames to open

    Returns a list of filenames or empty list if
    cancel button selected
    """

def askdirectory(
    *, initialdir: StrOrBytesPath | None = ..., mustexist: bool | None = False, parent: Misc | None = ..., title: str | None = ...
) -> str:  # can be empty string
    """Ask for a directory, and return the file name"""

# TODO: If someone actually uses these, overload to have the actual return type of open(..., mode)
def asksaveasfile(
    mode: str = "w",
    *,
    confirmoverwrite: bool | None = True,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> IO[Incomplete] | None:
    """Ask for a filename to save as, and returned the opened file"""

def askopenfile(
    mode: str = "r",
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> IO[Incomplete] | None:
    """Ask for a filename to open, and returned the opened file"""

def askopenfiles(
    mode: str = "r",
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> tuple[IO[Incomplete], ...]:  # can be empty tuple
    """Ask for multiple filenames and return the open file
    objects

    returns a list of open file objects or an empty list if
    cancel selected
    """

def test() -> None:
    """Simple test program."""
