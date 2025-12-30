"""This module tries to retrieve as much platform-identifying data as
possible. It makes this information available via function APIs.

If called from the command line, it prints the platform
information concatenated as single string to stdout. The output
format is usable as part of a filename.

"""

import sys
from typing import NamedTuple, type_check_only
from typing_extensions import Self, deprecated, disjoint_base

def libc_ver(executable: str | None = None, lib: str = "", version: str = "", chunksize: int = 16384) -> tuple[str, str]:
    """Tries to determine the libc version that the file executable
    (which defaults to the Python interpreter) is linked against.

    Returns a tuple of strings (lib,version) which default to the
    given parameters in case the lookup fails.

    Note that the function has intimate knowledge of how different
    libc versions add symbols to the executable and thus is probably
    only usable for executables compiled using gcc.

    The file is read and scanned in chunks of chunksize bytes.

    """

def win32_ver(release: str = "", version: str = "", csd: str = "", ptype: str = "") -> tuple[str, str, str, str]: ...
def win32_edition() -> str: ...
def win32_is_iot() -> bool: ...
def mac_ver(
    release: str = "", versioninfo: tuple[str, str, str] = ("", "", ""), machine: str = ""
) -> tuple[str, tuple[str, str, str], str]:
    """Get macOS version information and return it as tuple (release,
    versioninfo, machine) with versioninfo being a tuple (version,
    dev_stage, non_release_version).

    Entries which cannot be determined are set to the parameter values
    which default to ''. All tuple entries are strings.
    """

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13; will be removed in Python 3.15.")
    def java_ver(
        release: str = "",
        vendor: str = "",
        vminfo: tuple[str, str, str] = ("", "", ""),
        osinfo: tuple[str, str, str] = ("", "", ""),
    ) -> tuple[str, str, tuple[str, str, str], tuple[str, str, str]]:
        """Version interface for Jython.

        Returns a tuple (release, vendor, vminfo, osinfo) with vminfo being
        a tuple (vm_name, vm_release, vm_vendor) and osinfo being a
        tuple (os_name, os_version, os_arch).

        Values which cannot be determined are set to the defaults
        given as parameters (which all default to '').

        """

else:
    def java_ver(
        release: str = "",
        vendor: str = "",
        vminfo: tuple[str, str, str] = ("", "", ""),
        osinfo: tuple[str, str, str] = ("", "", ""),
    ) -> tuple[str, str, tuple[str, str, str], tuple[str, str, str]]:
        """Version interface for Jython.

        Returns a tuple (release, vendor, vminfo, osinfo) with vminfo being
        a tuple (vm_name, vm_release, vm_vendor) and osinfo being a
        tuple (os_name, os_version, os_arch).

        Values which cannot be determined are set to the defaults
        given as parameters (which all default to '').

        """

def system_alias(system: str, release: str, version: str) -> tuple[str, str, str]:
    """Returns (system, release, version) aliased to common
    marketing names used for some systems.

    It also does some reordering of the information in some cases
    where it would otherwise cause confusion.

    """

def architecture(executable: str = sys.executable, bits: str = "", linkage: str = "") -> tuple[str, str]:
    """Queries the given executable (defaults to the Python interpreter
    binary) for various architecture information.

    Returns a tuple (bits, linkage) which contains information about
    the bit architecture and the linkage format used for the
    executable. Both values are returned as strings.

    Values that cannot be determined are returned as given by the
    parameter presets. If bits is given as '', the sizeof(pointer)
    (or sizeof(long) on Python version < 1.5.2) is used as
    indicator for the supported pointer size.

    The function relies on the system's "file" command to do the
    actual work. This is available on most if not all Unix
    platforms. On some non-Unix platforms where the "file" command
    does not exist and the executable is set to the Python interpreter
    binary defaults from _default_architecture are used.

    """

# This class is not exposed. It calls itself platform.uname_result_base.
# At runtime it only has 5 fields.
@type_check_only
class _uname_result_base(NamedTuple):
    system: str
    node: str
    release: str
    version: str
    machine: str
    # This base class doesn't have this field at runtime, but claiming it
    # does is the least bad way to handle the situation. Nobody really
    # sees this class anyway. See #13068
    processor: str

# uname_result emulates a 6-field named tuple, but the processor field
# is lazily evaluated rather than being passed in to the constructor.
if sys.version_info >= (3, 12):
    class uname_result(_uname_result_base):
        """
        A uname_result that's largely compatible with a
        simple namedtuple except that 'processor' is
        resolved late and cached to avoid calling "uname"
        except when needed.
        """

        __match_args__ = ("system", "node", "release", "version", "machine")  # pyright: ignore[reportAssignmentType]

        def __new__(_cls, system: str, node: str, release: str, version: str, machine: str) -> Self:
            """Create new instance of uname_result_base(system, node, release, version, machine)"""

        @property
        def processor(self) -> str: ...

else:
    @disjoint_base
    class uname_result(_uname_result_base):
        """
        A uname_result that's largely compatible with a
        simple namedtuple except that 'processor' is
        resolved late and cached to avoid calling "uname"
        except when needed.
        """

        if sys.version_info >= (3, 10):
            __match_args__ = ("system", "node", "release", "version", "machine")  # pyright: ignore[reportAssignmentType]

        def __new__(_cls, system: str, node: str, release: str, version: str, machine: str) -> Self:
            """Create new instance of uname_result_base(system, node, release, version, machine)"""

        @property
        def processor(self) -> str: ...

def uname() -> uname_result:
    """Fairly portable uname interface. Returns a tuple
    of strings (system, node, release, version, machine, processor)
    identifying the underlying platform.

    Note that unlike the os.uname function this also returns
    possible processor information as an additional tuple entry.

    Entries which cannot be determined are set to ''.

    """

def system() -> str:
    """Returns the system/OS name, e.g. 'Linux', 'Windows' or 'Java'.

    An empty string is returned if the value cannot be determined.

    """

def node() -> str:
    """Returns the computer's network name (which may not be fully
    qualified)

    An empty string is returned if the value cannot be determined.

    """

def release() -> str:
    """Returns the system's release, e.g. '2.2.0' or 'NT'

    An empty string is returned if the value cannot be determined.

    """

def version() -> str:
    """Returns the system's release version, e.g. '#3 on degas'

    An empty string is returned if the value cannot be determined.

    """

def machine() -> str:
    """Returns the machine type, e.g. 'i386'

    An empty string is returned if the value cannot be determined.

    """

def processor() -> str:
    """Returns the (true) processor name, e.g. 'amdk6'

    An empty string is returned if the value cannot be
    determined. Note that many platforms do not provide this
    information or simply return the same value as for machine(),
    e.g.  NetBSD does this.

    """

def python_implementation() -> str:
    """Returns a string identifying the Python implementation.

    Currently, the following implementations are identified:
      'CPython' (C implementation of Python),
      'Jython' (Java implementation of Python),
      'PyPy' (Python implementation of Python).

    """

def python_version() -> str:
    """Returns the Python version as string 'major.minor.patchlevel'

    Note that unlike the Python sys.version, the returned value
    will always include the patchlevel (it defaults to 0).

    """

def python_version_tuple() -> tuple[str, str, str]:
    """Returns the Python version as tuple (major, minor, patchlevel)
    of strings.

    Note that unlike the Python sys.version, the returned value
    will always include the patchlevel (it defaults to 0).

    """

def python_branch() -> str:
    """Returns a string identifying the Python implementation
    branch.

    For CPython this is the SCM branch from which the
    Python binary was built.

    If not available, an empty string is returned.

    """

def python_revision() -> str:
    """Returns a string identifying the Python implementation
    revision.

    For CPython this is the SCM revision from which the
    Python binary was built.

    If not available, an empty string is returned.

    """

def python_build() -> tuple[str, str]:
    """Returns a tuple (buildno, builddate) stating the Python
    build number and date as strings.

    """

def python_compiler() -> str:
    """Returns a string identifying the compiler used for compiling
    Python.

    """

def platform(aliased: bool = False, terse: bool = False) -> str:
    """Returns a single string identifying the underlying platform
    with as much useful information as possible (but no more :).

    The output is intended to be human readable rather than
    machine parseable. It may look different on different
    platforms and this is intended.

    If "aliased" is true, the function will use aliases for
    various platforms that report system names which differ from
    their common names, e.g. SunOS will be reported as
    Solaris. The system_alias() function is used to implement
    this.

    Setting terse to true causes the function to return only the
    absolute minimum information needed to identify the platform.

    """

if sys.version_info >= (3, 10):
    def freedesktop_os_release() -> dict[str, str]:
        """Return operation system identification from freedesktop.org os-release"""

if sys.version_info >= (3, 13):
    class AndroidVer(NamedTuple):
        """AndroidVer(release, api_level, manufacturer, model, device, is_emulator)"""

        release: str
        api_level: int
        manufacturer: str
        model: str
        device: str
        is_emulator: bool

    class IOSVersionInfo(NamedTuple):
        """IOSVersionInfo(system, release, model, is_simulator)"""

        system: str
        release: str
        model: str
        is_simulator: bool

    def android_ver(
        release: str = "",
        api_level: int = 0,
        manufacturer: str = "",
        model: str = "",
        device: str = "",
        is_emulator: bool = False,
    ) -> AndroidVer: ...
    def ios_ver(system: str = "", release: str = "", model: str = "", is_simulator: bool = False) -> IOSVersionInfo:
        """Get iOS version information, and return it as a namedtuple:
            (system, release, model, is_simulator).

        If values can't be determined, they are set to values provided as
        parameters.
        """

if sys.version_info >= (3, 14):
    def invalidate_caches() -> None:
        """Invalidate the cached results."""
