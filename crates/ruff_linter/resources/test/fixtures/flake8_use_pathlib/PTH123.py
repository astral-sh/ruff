import builtins
from pathlib import Path

_file = "file.txt"
_x = ("r+", -1)
r_plus = "r+"

builtins.open(file=_file)

open(_file, "r+ ", -  1)
open(mode="wb", file=_file)
open(mode="r+", buffering=-1, file=_file, encoding="utf-8")
open(_file, "r+", - 1, None, None, None, True, None)
open(_file, "r+", -1, None, None, None, closefd=True, opener=None)
open(_file, mode="r+", buffering=-1, encoding=None, errors=None, newline=None)
open(_file, f"  {r_plus}      ", -  1)
open(buffering=-      1, file=_file, encoding=         "utf-8")

# Only diagnostic
open()
open(_file, *_x)
open(_file, "r+", unknown=True)
open(_file, "r+", closefd=False)
open(_file, "r+", None, None, None, None, None, None, None)