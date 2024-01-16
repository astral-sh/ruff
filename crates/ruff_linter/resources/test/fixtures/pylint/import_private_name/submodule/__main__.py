# Errors.
from _a import b
from c._d import e
from _f.g import h
from i import _j
from k import _l as m
import _aaa
import bbb.ccc._ddd as eee  # Panicked in https://github.com/astral-sh/ruff/pull/5920

# Non-errors.
import n
import o as _p
from q import r
from s import t as _v
from w.x import y
from z.aa import bb as _cc
from .dd import _ee  # Relative import.
from .ff._gg import hh  # Relative import.
from ._ii.jj import kk  # Relative import.
from __future__ import annotations  # __future__ is a special case.
from __main__ import main  # __main__ is a special case.
from ll import __version__  # __version__ is a special case.
from import_private_name import _top_level_secret  # Can import from self.
from import_private_name.submodule import _submodule_secret  # Can import from self.
from import_private_name.submodule.subsubmodule import (
    _subsubmodule_secret,
)  # Can import from self.

# Non-errors (used for type annotations).
from mm import _nn
from oo import _pp as qq
from _rr import ss
from tt._uu import vv
from _ww.xx import yy as zz
import _ddd as ddd

some_variable: _nn = None

def func(arg: qq) -> ss:
    pass

class Class:
    lst: list[ddd]

    def __init__(self, arg: vv) -> "zz":
        pass
