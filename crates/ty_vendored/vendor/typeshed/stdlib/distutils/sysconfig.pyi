"""Provide access to Python's configuration information.  The specific
configuration variables available depend heavily on the platform and
configuration.  The values may be retrieved using
get_config_var(name), and the list of variables is available via
get_config_vars().keys().  Additional convenience functions are also
available.

Written by:   Fred L. Drake, Jr.
Email:        <fdrake@acm.org>
"""

import sys
from collections.abc import Mapping
from distutils.ccompiler import CCompiler
from typing import Final, Literal, overload
from typing_extensions import deprecated

PREFIX: Final[str]
EXEC_PREFIX: Final[str]
BASE_PREFIX: Final[str]
BASE_EXEC_PREFIX: Final[str]
project_base: Final[str]
python_build: Final[bool]

def expand_makefile_vars(s: str, vars: Mapping[str, str]) -> str:
    """Expand Makefile-style variables -- "${foo}" or "$(foo)" -- in
    'string' according to 'vars' (a dictionary mapping variable names to
    values).  Variables not present in 'vars' are silently expanded to the
    empty string.  The variable values in 'vars' should not contain further
    variable expansions; if 'vars' is the output of 'parse_makefile()',
    you're fine.  Returns a variable-expanded version of 's'.
    """

@overload
@deprecated("SO is deprecated, use EXT_SUFFIX. Support is removed in Python 3.11")
def get_config_var(name: Literal["SO"]) -> int | str | None:
    """Return the value of a single variable using the dictionary returned by
    'get_config_vars()'.

    Equivalent to get_config_vars().get(name)
    """

@overload
def get_config_var(name: str) -> int | str | None: ...
@overload
def get_config_vars() -> dict[str, str | int]:
    """With no arguments, return a dictionary of all configuration
    variables relevant for the current platform.

    On Unix, this means every variable defined in Python's installed Makefile;
    On Windows it's a much smaller set.

    With arguments, return a list of values that result from looking up
    each argument in the configuration variable dictionary.
    """

@overload
def get_config_vars(arg: str, /, *args: str) -> list[str | int]: ...
def get_config_h_filename() -> str:
    """Return the path of pyconfig.h."""

def get_makefile_filename() -> str:
    """Return the path of the Makefile."""

def get_python_inc(plat_specific: bool | Literal[0, 1] = 0, prefix: str | None = None) -> str:
    """Return the directory containing installed Python header files.

    If 'plat_specific' is false (the default), this is the path to the
    non-platform-specific header files, i.e. Python.h and so on;
    otherwise, this is the path to platform-specific header files
    (namely pyconfig.h).

    If 'prefix' is supplied, use it instead of sys.base_prefix or
    sys.base_exec_prefix -- i.e., ignore 'plat_specific'.
    """

def get_python_lib(
    plat_specific: bool | Literal[0, 1] = 0, standard_lib: bool | Literal[0, 1] = 0, prefix: str | None = None
) -> str:
    """Return the directory containing the Python library (standard or
    site additions).

    If 'plat_specific' is true, return the directory containing
    platform-specific modules, i.e. any module from a non-pure-Python
    module distribution; otherwise, return the platform-shared library
    directory.  If 'standard_lib' is true, return the directory
    containing standard Python library modules; otherwise, return the
    directory for site-specific modules.

    If 'prefix' is supplied, use it instead of sys.base_prefix or
    sys.base_exec_prefix -- i.e., ignore 'plat_specific'.
    """

def customize_compiler(compiler: CCompiler) -> None:
    """Do any platform-specific customization of a CCompiler instance.

    Mainly needed on Unix, so we can plug in the information that
    varies across Unices and is stored in Python's Makefile.
    """

if sys.version_info < (3, 10):
    def get_python_version() -> str:
        """Return a string containing the major and minor Python version,
        leaving off the patchlevel.  Sample return values could be '1.5'
        or '2.2'.
        """
