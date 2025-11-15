"""Access to Python's configuration information."""

import sys
from typing import IO, Any, Literal, overload
from typing_extensions import LiteralString, deprecated

__all__ = [
    "get_config_h_filename",
    "get_config_var",
    "get_config_vars",
    "get_makefile_filename",
    "get_path",
    "get_path_names",
    "get_paths",
    "get_platform",
    "get_python_version",
    "get_scheme_names",
    "parse_config_h",
]

@overload
@deprecated("SO is deprecated, use EXT_SUFFIX. Support is removed in Python 3.11")
def get_config_var(name: Literal["SO"]) -> Any:
    """Return the value of a single variable using the dictionary returned by
    'get_config_vars()'.

    Equivalent to get_config_vars().get(name)
    """

@overload
def get_config_var(name: str) -> Any: ...
@overload
def get_config_vars() -> dict[str, Any]:
    """With no arguments, return a dictionary of all configuration
    variables relevant for the current platform.

    On Unix, this means every variable defined in Python's installed Makefile;
    On Windows it's a much smaller set.

    With arguments, return a list of values that result from looking up
    each argument in the configuration variable dictionary.
    """

@overload
def get_config_vars(arg: str, /, *args: str) -> list[Any]: ...
def get_scheme_names() -> tuple[str, ...]:
    """Return a tuple containing the schemes names."""

if sys.version_info >= (3, 10):
    def get_default_scheme() -> LiteralString: ...
    def get_preferred_scheme(key: Literal["prefix", "home", "user"]) -> LiteralString: ...
    # Documented -- see https://docs.python.org/3/library/sysconfig.html#sysconfig._get_preferred_schemes
    def _get_preferred_schemes() -> dict[Literal["prefix", "home", "user"], LiteralString]: ...

def get_path_names() -> tuple[str, ...]:
    """Return a tuple containing the paths names."""

def get_path(name: str, scheme: str = ..., vars: dict[str, Any] | None = None, expand: bool = True) -> str:
    """Return a path corresponding to the scheme.

    ``scheme`` is the install scheme name.
    """

def get_paths(scheme: str = ..., vars: dict[str, Any] | None = None, expand: bool = True) -> dict[str, str]:
    """Return a mapping containing an install scheme.

    ``scheme`` is the install scheme name. If not provided, it will
    return the default scheme for the current platform.
    """

def get_python_version() -> str: ...
def get_platform() -> str:
    """Return a string that identifies the current platform.

    This is used mainly to distinguish platform-specific build directories and
    platform-specific built distributions.  Typically includes the OS name and
    version and the architecture (as supplied by 'os.uname()'), although the
    exact information included depends on the OS; on Linux, the kernel version
    isn't particularly important.

    Examples of returned values:


    Windows:

    - win-amd64 (64-bit Windows on AMD64, aka x86_64, Intel64, and EM64T)
    - win-arm64 (64-bit Windows on ARM64, aka AArch64)
    - win32 (all others - specifically, sys.platform is returned)

    POSIX based OS:

    - linux-x86_64
    - macosx-15.5-arm64
    - macosx-26.0-universal2 (macOS on Apple Silicon or Intel)
    - android-24-arm64_v8a

    For other non-POSIX platforms, currently just returns :data:`sys.platform`.
    """

if sys.version_info >= (3, 11):
    def is_python_build(check_home: object = None) -> bool: ...

else:
    def is_python_build(check_home: bool = False) -> bool: ...

def parse_config_h(fp: IO[Any], vars: dict[str, Any] | None = None) -> dict[str, Any]:
    """Parse a config.h-style file.

    A dictionary containing name/value pairs is returned.  If an
    optional dictionary is passed in as the second argument, it is
    used instead of a new dictionary.
    """

def get_config_h_filename() -> str:
    """Return the path of pyconfig.h."""

def get_makefile_filename() -> str:
    """Return the path of the Makefile."""
