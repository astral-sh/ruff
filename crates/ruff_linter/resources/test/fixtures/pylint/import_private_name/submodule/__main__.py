# Errors.
from _a import b
from c._d import e
from _f.g import h
from i import _j
from k import _l as m

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
