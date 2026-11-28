## Banned modules ##
import cgi

from cgi import *

from cgi import a, b, c

# banning a module also bans any submodules
import cgi.foo.bar

from cgi.foo import bar

from cgi.foo.bar import *

## Banned module members ##

from typing import TypedDict

import typing

# attribute access is checked
typing.TypedDict

typing.TypedDict.anything

# function calls are checked
typing.TypedDict()

typing.TypedDict.anything()

# import aliases are resolved
import typing as totally_not_typing
totally_not_typing.TypedDict

# relative imports are respected
from .typing import TypedDict

# Only the implicit builtin matches the ban on .dict.fromkeys
import builtins

dict.fromkeys([])
builtins.dict.fromkeys([])

## Banned instance methods on annotated receivers (preview) ##

from pathlib import Path
from typing import Annotated

import pytest


def remove(path: Path) -> None:
    path.unlink()  # TID251 in preview
    Path.unlink(path)


def test_example(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("os.getcwd", lambda: "/")  # TID251 in preview
    pytest.MonkeyPatch.setattr(monkeypatch, "os.getcwd", lambda: "/")


def remove_annotated(path: Annotated[Path, "meta"]) -> None:
    path.unlink()  # TID251 in preview


def remove_assigned() -> None:
    target: Path = Path("file.txt")
    target.unlink()  # TID251 in preview


# Unannotated receivers are not flagged.
def remove_unannotated(path) -> None:
    path.unlink()


# Receivers annotated with an unrelated type are not flagged.
def remove_other(path: str) -> None:
    path.unlink()


# Non-banned methods on annotated receivers are not flagged.
def read(path: Path) -> None:
    path.read_text()
