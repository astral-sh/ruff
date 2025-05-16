# SPDX-License-Identifier: MIT
# SPDX-FileCopyrightText: 2021 Taneli Hukkinen
# Licensed to PSF under a Contributor Agreement.

__all__ = ("loads", "load", "TOMLDecodeError")

from ._parser import TOMLDecodeError, load, loads

# Pretend this exception was created here.
TOMLDecodeError.__module__ = __name__

# Dummy function to reproduce a performance issue from astral-sh/ty#431
from pathlib import Path

def func(p: Path) -> None:
    p.read_bytes()
