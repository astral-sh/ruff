from tkinter import Misc
from tkinter.commondialog import Dialog
from typing import ClassVar, Final, Literal

__all__ = ["showinfo", "showwarning", "showerror", "askquestion", "askokcancel", "askyesno", "askyesnocancel", "askretrycancel"]

ERROR: Final = "error"
INFO: Final = "info"
QUESTION: Final = "question"
WARNING: Final = "warning"
ABORTRETRYIGNORE: Final = "abortretryignore"
OK: Final = "ok"
OKCANCEL: Final = "okcancel"
RETRYCANCEL: Final = "retrycancel"
YESNO: Final = "yesno"
YESNOCANCEL: Final = "yesnocancel"
ABORT: Final = "abort"
RETRY: Final = "retry"
IGNORE: Final = "ignore"
CANCEL: Final = "cancel"
YES: Final = "yes"
NO: Final = "no"

class Message(Dialog):
    command: ClassVar[str]

def showinfo(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["ok"] = "ok",
    parent: Misc = ...,
) -> str: ...
def showwarning(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["ok"] = "ok",
    parent: Misc = ...,
) -> str: ...
def showerror(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["ok"] = "ok",
    parent: Misc = ...,
) -> str: ...
def askquestion(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["yes", "no"] = ...,
    parent: Misc = ...,
) -> str: ...
def askokcancel(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["ok", "cancel"] = ...,
    parent: Misc = ...,
) -> bool: ...
def askyesno(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["yes", "no"] = ...,
    parent: Misc = ...,
) -> bool: ...
def askyesnocancel(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["cancel", "yes", "no"] = ...,
    parent: Misc = ...,
) -> bool | None: ...
def askretrycancel(
    title: str | None = None,
    message: str | None = None,
    *,
    detail: str = ...,
    icon: Literal["error", "info", "question", "warning"] = ...,
    default: Literal["retry", "cancel"] = ...,
    parent: Misc = ...,
) -> bool: ...
