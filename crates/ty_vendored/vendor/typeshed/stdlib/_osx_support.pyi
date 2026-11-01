"""Shared OS X support functions."""

from collections.abc import Iterable, Sequence
from typing import Final, TypeVar

_T = TypeVar("_T")
_K = TypeVar("_K")
_V = TypeVar("_V")

__all__ = ["compiler_fixup", "customize_config_vars", "customize_compiler", "get_platform_osx"]

_UNIVERSAL_CONFIG_VARS: Final[tuple[str, ...]]  # undocumented
_COMPILER_CONFIG_VARS: Final[tuple[str, ...]]  # undocumented
_INITPRE: Final[str]  # undocumented

def _find_executable(executable: str, path: str | None = None) -> str | None:  # undocumented
    """Tries to find 'executable' in the directories listed in 'path'.

    A string listing directories separated by 'os.pathsep'; defaults to
    os.environ['PATH'].  Returns the complete filename or None if not found.
    """

def _read_output(commandstring: str, capture_stderr: bool = False) -> str | None:  # undocumented
    """Output from successful command execution or None"""

def _find_build_tool(toolname: str) -> str:  # undocumented
    """Find a build tool on current path or using xcrun"""

_SYSTEM_VERSION: Final[str | None]  # undocumented

def _get_system_version() -> str:  # undocumented
    """Return the OS X system version as a string"""

def _remove_original_values(_config_vars: dict[str, str]) -> None:  # undocumented
    """Remove original unmodified values for testing"""

def _save_modified_value(_config_vars: dict[str, str], cv: str, newvalue: str) -> None:  # undocumented
    """Save modified and original unmodified value of configuration var"""

def _supports_universal_builds() -> bool:  # undocumented
    """Returns True if universal builds are supported on this system"""

def _find_appropriate_compiler(_config_vars: dict[str, str]) -> dict[str, str]:  # undocumented
    """Find appropriate C compiler for extension module builds"""

def _remove_universal_flags(_config_vars: dict[str, str]) -> dict[str, str]:  # undocumented
    """Remove all universal build arguments from config vars"""

def _remove_unsupported_archs(_config_vars: dict[str, str]) -> dict[str, str]:  # undocumented
    """Remove any unsupported archs from config vars"""

def _override_all_archs(_config_vars: dict[str, str]) -> dict[str, str]:  # undocumented
    """Allow override of all archs with ARCHFLAGS env var"""

def _check_for_unavailable_sdk(_config_vars: dict[str, str]) -> dict[str, str]:  # undocumented
    """Remove references to any SDKs not available"""

def compiler_fixup(compiler_so: Iterable[str], cc_args: Sequence[str]) -> list[str]:
    """
    This function will strip '-isysroot PATH' and '-arch ARCH' from the
    compile flags if the user has specified one them in extra_compile_flags.

    This is needed because '-arch ARCH' adds another architecture to the
    build, without a way to remove an architecture. Furthermore GCC will
    barf if multiple '-isysroot' arguments are present.
    """

def customize_config_vars(_config_vars: dict[str, str]) -> dict[str, str]:
    """Customize Python build configuration variables.

    Called internally from sysconfig with a mutable mapping
    containing name/value pairs parsed from the configured
    makefile used to build this interpreter.  Returns
    the mapping updated as needed to reflect the environment
    in which the interpreter is running; in the case of
    a Python from a binary installer, the installed
    environment may be very different from the build
    environment, i.e. different OS levels, different
    built tools, different available CPU architectures.

    This customization is performed whenever
    distutils.sysconfig.get_config_vars() is first
    called.  It may be used in environments where no
    compilers are present, i.e. when installing pure
    Python dists.  Customization of compiler paths
    and detection of unavailable archs is deferred
    until the first extension module build is
    requested (in distutils.sysconfig.customize_compiler).

    Currently called from distutils.sysconfig
    """

def customize_compiler(_config_vars: dict[str, str]) -> dict[str, str]:
    """Customize compiler path and configuration variables.

    This customization is performed when the first
    extension module build is requested
    in distutils.sysconfig.customize_compiler.
    """

def get_platform_osx(_config_vars: dict[str, str], osname: _T, release: _K, machine: _V) -> tuple[str | _T, str | _K, str | _V]:
    """Filter values for get_platform()"""
