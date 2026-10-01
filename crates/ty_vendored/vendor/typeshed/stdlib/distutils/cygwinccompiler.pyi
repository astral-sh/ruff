"""distutils.cygwinccompiler

Provides the CygwinCCompiler class, a subclass of UnixCCompiler that
handles the Cygwin port of the GNU C compiler to Windows.  It also contains
the Mingw32CCompiler class which handles the mingw32 port of GCC (same as
cygwin in no-cygwin mode).
"""

from distutils.unixccompiler import UnixCCompiler
from distutils.version import LooseVersion
from re import Pattern
from typing import Final, Literal

def get_msvcr() -> list[str] | None:
    """Include the appropriate MSVC runtime library if Python was built
    with MSVC 7.0 or later.
    """

class CygwinCCompiler(UnixCCompiler):
    """Handles the Cygwin port of the GNU C compiler to Windows."""

class Mingw32CCompiler(CygwinCCompiler):
    """Handles the Mingw32 port of the GNU C compiler to Windows."""

CONFIG_H_OK: Final = "ok"
CONFIG_H_NOTOK: Final = "not ok"
CONFIG_H_UNCERTAIN: Final = "uncertain"

def check_config_h() -> tuple[Literal["ok", "not ok", "uncertain"], str]:
    """Check if the current Python installation appears amenable to building
    extensions with GCC.

    Returns a tuple (status, details), where 'status' is one of the following
    constants:

    - CONFIG_H_OK: all is well, go ahead and compile
    - CONFIG_H_NOTOK: doesn't look good
    - CONFIG_H_UNCERTAIN: not sure -- unable to read pyconfig.h

    'details' is a human-readable string explaining the situation.

    Note there are two ways to conclude "OK": either 'sys.version' contains
    the string "GCC" (implying that this Python was built with GCC), or the
    installed "pyconfig.h" contains the string "__GNUC__".
    """

RE_VERSION: Final[Pattern[bytes]]

def get_versions() -> tuple[LooseVersion | None, ...]:
    """Try to find out the versions of gcc, ld and dllwrap.

    If not possible it returns None for it.
    """

def is_cygwingcc() -> bool:
    """Try to determine if the gcc that would be used is from cygwin."""
