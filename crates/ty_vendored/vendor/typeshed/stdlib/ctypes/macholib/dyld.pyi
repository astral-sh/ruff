"""
dyld emulation
"""

from collections.abc import Mapping
from ctypes.macholib.dylib import dylib_info as dylib_info
from ctypes.macholib.framework import framework_info as framework_info

__all__ = ["dyld_find", "framework_find", "framework_info", "dylib_info"]

def dyld_find(name: str, executable_path: str | None = None, env: Mapping[str, str] | None = None) -> str:
    """
    Find a library or framework using dyld semantics
    """

def framework_find(fn: str, executable_path: str | None = None, env: Mapping[str, str] | None = None) -> str:
    """
    Find a framework using dyld semantics in a very loose manner.

    Will take input such as:
        Python
        Python.framework
        Python.framework/Versions/Current
    """
