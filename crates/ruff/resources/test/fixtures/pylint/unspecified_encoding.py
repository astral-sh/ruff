"""Warnings for using open() without specifying an encoding"""
import dataclasses
import io
import locale
from pathlib import Path
from typing import Optional

FILENAME = "foo.bar"
open(FILENAME, "w", encoding="utf-8")
open(FILENAME, "wb")
open(FILENAME, "w+b")
open(FILENAME)  # [unspecified-encoding]
open(FILENAME, "wt")  # [unspecified-encoding]
open(FILENAME, "w+")  # [unspecified-encoding]
open(FILENAME, "w", encoding=None)  # [unspecified-encoding]
open(FILENAME, "r")  # [unspecified-encoding]

with open(FILENAME, encoding="utf8", errors="surrogateescape") as f:
    pass

LOCALE_ENCODING = locale.getlocale()[1]
with open(FILENAME, encoding=LOCALE_ENCODING) as f:
    pass

with open(FILENAME) as f:  # [unspecified-encoding]
    pass

with open(FILENAME, encoding=None) as f:  # [unspecified-encoding]
    pass

LOCALE_ENCODING = None
with open(FILENAME, encoding=LOCALE_ENCODING) as f:  # [unspecified-encoding]
    pass

io.open(FILENAME, "w+b")
io.open_code(FILENAME)
io.open(FILENAME)  # [unspecified-encoding]
io.open(FILENAME, "wt")  # [unspecified-encoding]
io.open(FILENAME, "w+")  # [unspecified-encoding]
io.open(FILENAME, "w", encoding=None)  # [unspecified-encoding]

with io.open(FILENAME, encoding="utf8", errors="surrogateescape") as f:
    pass

LOCALE_ENCODING = locale.getlocale()[1]
with io.open(FILENAME, encoding=LOCALE_ENCODING) as f:
    pass

with io.open(FILENAME) as f:  # [unspecified-encoding]
    pass

with io.open(FILENAME, encoding=None) as f:  # [unspecified-encoding]
    pass

LOCALE_ENCODING = None
with io.open(FILENAME, encoding=LOCALE_ENCODING) as f:  # [unspecified-encoding]
    pass

LOCALE_ENCODING = locale.getlocale()[1]
Path(FILENAME).read_text(encoding=LOCALE_ENCODING)
Path(FILENAME).read_text(encoding="utf8")
Path(FILENAME).read_text("utf8")

LOCALE_ENCODING = None
Path(FILENAME).read_text()  # [unspecified-encoding]
Path(FILENAME).read_text(encoding=None)  # [unspecified-encoding]
Path(FILENAME).read_text(encoding=LOCALE_ENCODING)  # [unspecified-encoding]

LOCALE_ENCODING = locale.getlocale()[1]
Path(FILENAME).write_text("string", encoding=LOCALE_ENCODING)
Path(FILENAME).write_text("string", encoding="utf8")

LOCALE_ENCODING = None
Path(FILENAME).write_text("string")  # [unspecified-encoding]
Path(FILENAME).write_text("string", encoding=None)  # [unspecified-encoding]
Path(FILENAME).write_text("string", encoding=LOCALE_ENCODING)  # [unspecified-encoding]

LOCALE_ENCODING = locale.getlocale()[1]
Path(FILENAME).open("w+b")
Path(FILENAME).open()  # [unspecified-encoding]
Path(FILENAME).open("wt")  # [unspecified-encoding]
Path(FILENAME).open("w+")  # [unspecified-encoding]
Path(FILENAME).open("w", encoding=None)  # [unspecified-encoding]
Path(FILENAME).open("w", encoding=LOCALE_ENCODING)


# Tests for storing data about open calls.
# Most of these are regression tests for a crash
# reported in https://github.com/PyCQA/pylint/issues/5321

# -- Constants
MODE = "wb"
open(FILENAME, mode=MODE)


# -- Functions
def return_mode_function():
    """Return a mode for open call"""
    return "wb"

open(FILENAME, mode=return_mode_function())


# -- Classes
class IOData:
    """Class that returns mode strings"""

    mode = "wb"

    def __init__(self):
        self.my_mode = "wb"

    @staticmethod
    def my_mode_method():
        """Returns a pre-defined mode"""
        return "wb"

    @staticmethod
    def my_mode_method_returner(mode: str) -> str:
        """Returns the supplied mode"""
        return mode


open(FILENAME, mode=IOData.mode)
open(FILENAME, mode=IOData().my_mode)
open(FILENAME, mode=IOData().my_mode_method())
open(FILENAME, mode=IOData().my_mode_method_returner("wb"))
# Invalid value but shouldn't crash, reported in https://github.com/PyCQA/pylint/issues/5321
open(FILENAME, mode=IOData)


# -- Dataclasses
@dataclasses.dataclass
class IOArgs:
    """Dataclass storing information about how to open a file"""

    encoding: Optional[str]
    mode: str


args_good_one = IOArgs(encoding=None, mode="wb")

# Test for crash reported in https://github.com/PyCQA/pylint/issues/5321
open(FILENAME, args_good_one.mode, encoding=args_good_one.encoding)

# Positional arguments
open(FILENAME, "w", -1, "utf-8")
open(FILENAME, "w", -1)  # [unspecified-encoding]

Path(FILENAME).open("w", -1, "utf-8")
Path(FILENAME).open("w", -1)  # [unspecified-encoding]

Path(FILENAME).read_text("utf-8")
Path(FILENAME).read_text()  # [unspecified-encoding]

Path(FILENAME).write_text("string", "utf-8")
Path(FILENAME).write_text("string")  # [unspecified-encoding]

# Test for crash reported in https://github.com/PyCQA/pylint/issues/5731
open(FILENAME, mode=None)  # [bad-open-mode, unspecified-encoding]

# Test for crash reported in https://github.com/PyCQA/pylint/issues/6414
open('foo', mode=2)  # [bad-open-mode, unspecified-encoding]