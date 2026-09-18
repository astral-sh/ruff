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


def generic[T: _nn](arg: T) -> T:
    return arg

from foo.    _bar import baz

# PLC2701 exceptions: `os._exit` is considered public despite leading underscore.
from os import _exit
from os import _exit as process_exit
from another_module import _exit as another_exit
from os import _private_member
from os import _exit as os_exit, _other_private_member

# PLC2701 exceptions: more underscore-prefixed standard library members are
# considered public despite their leading underscores.
from __future__ import _Feature
from asyncio import _enter_task
from asyncio import _leave_task
from asyncio import _register_task
from asyncio import _unregister_task
from ctypes import _CFuncPtr
from sys import _emscripten_info
from sys import _enablelegacywindowsfsencoding
from importlib.util import _incompatible_extension_module_restrictions
from ssl import _create_unverified_context
from subprocess import _USE_POSIX_SPAWN
from subprocess import _USE_VFORK
from sys import _clear_internal_caches
from sys import _clear_type_cache
from sys import _current_exceptions
from sys import _current_frames
from sys import _stats_clear
from sys import _stats_dump
from sys import _stats_off
from sys import _stats_on
from ctypes import _CData
from ctypes import _Pointer
from ctypes import _SimpleCData
from sysconfig import _get_preferred_schemes
from sys import _debugmallocstats
from sys import _getframe
from sys import _getframemodulename
from sys import _is_gil_enabled
from sys import _is_immortal
from sys import _is_interned
from sys import _jit
from sys import _xoptions
from logging import _defaultFormatter
from sys import _base_executable
from sys.implementation import _multiarch

# PLC2701 errors: standard library members that are not documented.
from sys import _private_member_again
from sys import _getframe as frame_getter, _undeclared_private_member
