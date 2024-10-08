import sys
from tkinter.commondialog import Dialog
from typing import ClassVar, Final

if sys.version_info >= (3, 9):
    __all__ = [
        "showinfo",
        "showwarning",
        "showerror",
        "askquestion",
        "askokcancel",
        "askyesno",
        "askyesnocancel",
        "askretrycancel",
    ]

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

def showinfo(title: str | None = None, message: str | None = None, **options) -> str: ...
def showwarning(title: str | None = None, message: str | None = None, **options) -> str: ...
def showerror(title: str | None = None, message: str | None = None, **options) -> str: ...
def askquestion(title: str | None = None, message: str | None = None, **options) -> str: ...
def askokcancel(title: str | None = None, message: str | None = None, **options) -> bool: ...
def askyesno(title: str | None = None, message: str | None = None, **options) -> bool: ...
def askyesnocancel(title: str | None = None, message: str | None = None, **options) -> bool | None: ...
def askretrycancel(title: str | None = None, message: str | None = None, **options) -> bool: ...
