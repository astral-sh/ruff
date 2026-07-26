# Issue: https://github.com/astral-sh/ruff/issues/26160

import sys

from mypkg import supported

assert sys.platform == "darwin"

import objc

assert sys.platform != "win32", "not supported on Windows"

import fcntl

assert sys.platform.startswith("linux")

import grp

assert sys.platform != "win32" and sys.platform != "cygwin"

import pwd

assert not (sys.platform == "win32" or sys.platform == "cygwin")

import resource

# The `if`-guarded equivalents of the assertions above. These are already exempt, because a
# top-level `if` block never ends the import section, but they're the forms the assertions are
# meant to be interchangeable with, so pin them down here too.
if sys.platform != "darwin":
    raise OSError

import AppKit

if sys.platform != "darwin":
    assert False

import asyncio

if sys.platform != "darwin":
    raise OSError("macOS only")
else:
    pass

import Quartz

# An `assert` that merely mentions `sys.platform` is not a platform check: a type checker won't
# narrow on it, and it can have side effects. So it ends the import block like any other statement.
assert supported(sys.platform)

import ctypes
