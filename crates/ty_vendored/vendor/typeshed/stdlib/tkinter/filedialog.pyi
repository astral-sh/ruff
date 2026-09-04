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
    title: str
    def ok_command(self) -> None: ...

class SaveFileDialog(FileDialog):
    title: str
    def ok_command(self) -> None: ...

class _Dialog(commondialog.Dialog): ...

class Open(_Dialog):
    command: ClassVar[str]

class SaveAs(_Dialog):
    command: ClassVar[str]

class Directory(commondialog.Dialog):
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
) -> str: ...  # can be empty string
def askopenfilename(
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> str: ...  # can be empty string
def askopenfilenames(
    *,
    defaultextension: str | None = "",
    filetypes: Iterable[tuple[str, str | list[str] | tuple[str, ...]]] | None = ...,
    initialdir: StrOrBytesPath | None = ...,
    initialfile: StrOrBytesPath | None = ...,
    parent: Misc | None = ...,
    title: str | None = ...,
    typevariable: StringVar | str | None = ...,
) -> Literal[""] | tuple[str, ...]: ...
def askdirectory(
    *, initialdir: StrOrBytesPath | None = ..., mustexist: bool | None = False, parent: Misc | None = ..., title: str | None = ...
) -> str: ...  # can be empty string

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
) -> IO[Incomplete] | None: ...
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
) -> IO[Incomplete] | None: ...
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
) -> tuple[IO[Incomplete], ...]: ...  # can be empty tuple
def test() -> None: ...
