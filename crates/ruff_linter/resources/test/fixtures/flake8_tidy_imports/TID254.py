from __future__ import annotations

import os
import typing as t
lazy import pathlib

from foo import bar
lazy from foo import baz
from starry import *

with manager():
    import email

if True:
    from bar import qux

try:
    import collections
except Exception:
    pass

def func():
    import fractions

class Example:
    import decimal

x = 1; import pkg.submodule
