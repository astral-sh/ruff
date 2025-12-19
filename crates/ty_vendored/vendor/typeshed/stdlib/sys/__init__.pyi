"""This module provides access to some objects used or maintained by the
interpreter and to functions that interact strongly with the interpreter.

Dynamic objects:

argv -- command line arguments; argv[0] is the script pathname if known
path -- module search path; path[0] is the script directory, else ''
modules -- dictionary of loaded modules

displayhook -- called to show results in an interactive session
excepthook -- called to handle any uncaught exception other than SystemExit
  To customize printing in an interactive session or to install a custom
  top-level exception handler, assign other functions to replace these.

stdin -- standard input file object; used by input()
stdout -- standard output file object; used by print()
stderr -- standard error object; used for error messages
  By assigning other file objects (or objects that behave like files)
  to these, it is possible to redirect all of the interpreter's I/O.

last_exc - the last uncaught exception
  Only available in an interactive session after a
  traceback has been printed.
last_type -- type of last uncaught exception
last_value -- value of last uncaught exception
last_traceback -- traceback of last uncaught exception
  These three are the (deprecated) legacy representation of last_exc.

Static objects:

builtin_module_names -- tuple of module names built into this interpreter
copyright -- copyright notice pertaining to this interpreter
exec_prefix -- prefix used to find the machine-specific Python library
executable -- absolute path of the executable binary of the Python interpreter
float_info -- a named tuple with information about the float implementation.
float_repr_style -- string indicating the style of repr() output for floats
hash_info -- a named tuple with information about the hash algorithm.
hexversion -- version information encoded as a single integer
implementation -- Python implementation information.
int_info -- a named tuple with information about the int implementation.
maxsize -- the largest supported length of containers.
maxunicode -- the value of the largest Unicode code point
platform -- platform identifier
prefix -- prefix used to find the Python library
thread_info -- a named tuple with information about the thread implementation.
version -- the version of this interpreter as a string
version_info -- version information as a named tuple
__stdin__ -- the original stdin; don't touch!
__stdout__ -- the original stdout; don't touch!
__stderr__ -- the original stderr; don't touch!
__displayhook__ -- the original displayhook; don't touch!
__excepthook__ -- the original excepthook; don't touch!

Functions:

displayhook() -- print an object to the screen, and save it in builtins._
excepthook() -- print an exception and its traceback to sys.stderr
exception() -- return the current thread's active exception
exc_info() -- return information about the current thread's active exception
exit() -- exit the interpreter by raising SystemExit
getdlopenflags() -- returns flags to be used for dlopen() calls
getprofile() -- get the global profiling function
getrefcount() -- return the reference count for an object (plus one :-)
getrecursionlimit() -- return the max recursion depth for the interpreter
getsizeof() -- return the size of an object in bytes
gettrace() -- get the global debug tracing function
setdlopenflags() -- set the flags to be used for dlopen() calls
setprofile() -- set the global profiling function
setrecursionlimit() -- set the max recursion depth for the interpreter
settrace() -- set the global debug tracing function
"""

import sys
from _typeshed import MaybeNone, OptExcInfo, ProfileFunction, StrOrBytesPath, TraceFunction, structseq
from _typeshed.importlib import MetaPathFinderProtocol, PathEntryFinderProtocol
from builtins import object as _object
from collections.abc import AsyncGenerator, Callable, Sequence
from io import TextIOWrapper
from types import FrameType, ModuleType, TracebackType
from typing import Any, Final, Literal, NoReturn, Protocol, TextIO, TypeVar, final, overload, type_check_only
from typing_extensions import LiteralString, TypeAlias, deprecated

_T = TypeVar("_T")

# see https://github.com/python/typeshed/issues/8513#issue-1333671093 for the rationale behind this alias
_ExitCode: TypeAlias = str | int | None

# ----- sys variables -----
if sys.platform != "win32":
    abiflags: str
argv: list[str]
base_exec_prefix: str
base_prefix: str
byteorder: Literal["little", "big"]
builtin_module_names: Sequence[str]  # actually a tuple of strings
copyright: str
if sys.platform == "win32":
    dllhandle: int
dont_write_bytecode: bool
displayhook: Callable[[object], Any]
excepthook: Callable[[type[BaseException], BaseException, TracebackType | None], Any]
exec_prefix: str
executable: str
float_repr_style: Literal["short", "legacy"]
hexversion: int
last_type: type[BaseException] | None
last_value: BaseException | None
last_traceback: TracebackType | None
if sys.version_info >= (3, 12):
    last_exc: BaseException  # or undefined.
maxsize: int
maxunicode: int
meta_path: list[MetaPathFinderProtocol]
modules: dict[str, ModuleType]
if sys.version_info >= (3, 10):
    orig_argv: list[str]
path: list[str]
path_hooks: list[Callable[[str], PathEntryFinderProtocol]]
path_importer_cache: dict[str, PathEntryFinderProtocol | None]
platform: LiteralString
platlibdir: str
prefix: str
pycache_prefix: str | None
ps1: object
ps2: object

# TextIO is used instead of more specific types for the standard streams,
# since they are often monkeypatched at runtime. At startup, the objects
# are initialized to instances of TextIOWrapper, but can also be None under
# some circumstances.
#
# To use methods from TextIOWrapper, use an isinstance check to ensure that
# the streams have not been overridden:
#
# if isinstance(sys.stdout, io.TextIOWrapper):
#    sys.stdout.reconfigure(...)
stdin: TextIO | MaybeNone
stdout: TextIO | MaybeNone
stderr: TextIO | MaybeNone

if sys.version_info >= (3, 10):
    stdlib_module_names: frozenset[str]

__stdin__: Final[TextIOWrapper | None]  # Contains the original value of stdin
__stdout__: Final[TextIOWrapper | None]  # Contains the original value of stdout
__stderr__: Final[TextIOWrapper | None]  # Contains the original value of stderr
tracebacklimit: int | None
version: str
api_version: int
warnoptions: Any
#  Each entry is a tuple of the form (action, message, category, module,
#    lineno)
if sys.platform == "win32":
    winver: str
_xoptions: dict[Any, Any]

# Type alias used as a mixin for structseq classes that cannot be instantiated at runtime
# This can't be represented in the type system, so we just use `structseq[Any]`
_UninstantiableStructseq: TypeAlias = structseq[Any]

flags: _flags

# This class is not exposed at runtime. It calls itself sys.flags.
# As a tuple, it can have a length between 15 and 18. We don't model
# the exact length here because that varies by patch version due to
# the backported security fix int_max_str_digits. The exact length shouldn't
# be relied upon. See #13031
# This can be re-visited when typeshed drops support for 3.10,
# at which point all supported versions will include int_max_str_digits
# in all patch versions.
# 3.9 is 15 or 16-tuple
# 3.10 is 16 or 17-tuple
# 3.11+ is an 18-tuple.
@final
@type_check_only
class _flags(_UninstantiableStructseq, tuple[int, ...]):
    """sys.flags

    Flags provided through command line arguments or environment vars.
    """

    # `safe_path` was added in py311
    if sys.version_info >= (3, 11):
        __match_args__: Final = (
            "debug",
            "inspect",
            "interactive",
            "optimize",
            "dont_write_bytecode",
            "no_user_site",
            "no_site",
            "ignore_environment",
            "verbose",
            "bytes_warning",
            "quiet",
            "hash_randomization",
            "isolated",
            "dev_mode",
            "utf8_mode",
            "warn_default_encoding",
            "safe_path",
            "int_max_str_digits",
        )
    elif sys.version_info >= (3, 10):
        __match_args__: Final = (
            "debug",
            "inspect",
            "interactive",
            "optimize",
            "dont_write_bytecode",
            "no_user_site",
            "no_site",
            "ignore_environment",
            "verbose",
            "bytes_warning",
            "quiet",
            "hash_randomization",
            "isolated",
            "dev_mode",
            "utf8_mode",
            "warn_default_encoding",
            "int_max_str_digits",
        )

    @property
    def debug(self) -> int:
        """-d"""

    @property
    def inspect(self) -> int:
        """-i"""

    @property
    def interactive(self) -> int:
        """-i"""

    @property
    def optimize(self) -> int:
        """-O or -OO"""

    @property
    def dont_write_bytecode(self) -> int:
        """-B"""

    @property
    def no_user_site(self) -> int:
        """-s"""

    @property
    def no_site(self) -> int:
        """-S"""

    @property
    def ignore_environment(self) -> int:
        """-E"""

    @property
    def verbose(self) -> int:
        """-v"""

    @property
    def bytes_warning(self) -> int:
        """-b"""

    @property
    def quiet(self) -> int:
        """-q"""

    @property
    def hash_randomization(self) -> int:
        """-R"""

    @property
    def isolated(self) -> int:
        """-I"""

    @property
    def dev_mode(self) -> bool:
        """-X dev"""

    @property
    def utf8_mode(self) -> int:
        """-X utf8"""
    if sys.version_info >= (3, 10):
        @property
        def warn_default_encoding(self) -> int:
            """-X warn_default_encoding"""
    if sys.version_info >= (3, 11):
        @property
        def safe_path(self) -> bool:
            """-P"""
    if sys.version_info >= (3, 13):
        @property
        def gil(self) -> Literal[0, 1]:
            """-X gil"""
    if sys.version_info >= (3, 14):
        @property
        def thread_inherit_context(self) -> Literal[0, 1]:
            """-X thread_inherit_context"""

        @property
        def context_aware_warnings(self) -> Literal[0, 1]:
            """-X context_aware_warnings"""
    # Whether or not this exists on lower versions of Python
    # may depend on which patch release you're using
    # (it was backported to all Python versions on 3.8+ as a security fix)
    # Added in: 3.9.14, 3.10.7
    # and present in all versions of 3.11 and later.
    @property
    def int_max_str_digits(self) -> int:
        """-X int_max_str_digits"""

float_info: _float_info

# This class is not exposed at runtime. It calls itself sys.float_info.
@final
@type_check_only
class _float_info(structseq[float], tuple[float, int, int, float, int, int, int, int, float, int, int]):
    """sys.float_info

    A named tuple holding information about the float type. It contains low level
    information about the precision and internal representation. Please study
    your system's :file:`float.h` for more information.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = (
            "max",
            "max_exp",
            "max_10_exp",
            "min",
            "min_exp",
            "min_10_exp",
            "dig",
            "mant_dig",
            "epsilon",
            "radix",
            "rounds",
        )

    @property
    def max(self) -> float:  # DBL_MAX
        """DBL_MAX -- maximum representable finite float"""

    @property
    def max_exp(self) -> int:  # DBL_MAX_EXP
        """DBL_MAX_EXP -- maximum int e such that radix**(e-1) is representable"""

    @property
    def max_10_exp(self) -> int:  # DBL_MAX_10_EXP
        """DBL_MAX_10_EXP -- maximum int e such that 10**e is representable"""

    @property
    def min(self) -> float:  # DBL_MIN
        """DBL_MIN -- Minimum positive normalized float"""

    @property
    def min_exp(self) -> int:  # DBL_MIN_EXP
        """DBL_MIN_EXP -- minimum int e such that radix**(e-1) is a normalized float"""

    @property
    def min_10_exp(self) -> int:  # DBL_MIN_10_EXP
        """DBL_MIN_10_EXP -- minimum int e such that 10**e is a normalized float"""

    @property
    def dig(self) -> int:  # DBL_DIG
        """DBL_DIG -- maximum number of decimal digits that can be faithfully represented in a float"""

    @property
    def mant_dig(self) -> int:  # DBL_MANT_DIG
        """DBL_MANT_DIG -- mantissa digits"""

    @property
    def epsilon(self) -> float:  # DBL_EPSILON
        """DBL_EPSILON -- Difference between 1 and the next representable float"""

    @property
    def radix(self) -> int:  # FLT_RADIX
        """FLT_RADIX -- radix of exponent"""

    @property
    def rounds(self) -> int:  # FLT_ROUNDS
        """FLT_ROUNDS -- rounding mode used for arithmetic operations"""

hash_info: _hash_info

# This class is not exposed at runtime. It calls itself sys.hash_info.
@final
@type_check_only
class _hash_info(structseq[Any | int], tuple[int, int, int, int, int, str, int, int, int]):
    """hash_info

    A named tuple providing parameters used for computing
    hashes. The attributes are read only.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("width", "modulus", "inf", "nan", "imag", "algorithm", "hash_bits", "seed_bits", "cutoff")

    @property
    def width(self) -> int:
        """width of the type used for hashing, in bits"""

    @property
    def modulus(self) -> int:
        """prime number giving the modulus on which the hash function is based"""

    @property
    def inf(self) -> int:
        """value to be used for hash of a positive infinity"""

    @property
    def nan(self) -> int:
        """value to be used for hash of a nan"""

    @property
    def imag(self) -> int:
        """multiplier used for the imaginary part of a complex number"""

    @property
    def algorithm(self) -> str:
        """name of the algorithm for hashing of str, bytes and memoryviews"""

    @property
    def hash_bits(self) -> int:
        """internal output size of hash algorithm"""

    @property
    def seed_bits(self) -> int:
        """seed size of hash algorithm"""

    @property
    def cutoff(self) -> int:  # undocumented
        """small string optimization cutoff"""

implementation: _implementation

# This class isn't really a thing. At runtime, implementation is an instance
# of types.SimpleNamespace. This allows for better typing.
@type_check_only
class _implementation:
    name: str
    version: _version_info
    hexversion: int
    cache_tag: str
    # Define __getattr__, as the documentation states:
    # > sys.implementation may contain additional attributes specific to the Python implementation.
    # > These non-standard attributes must start with an underscore, and are not described here.
    def __getattr__(self, name: str) -> Any: ...

int_info: _int_info

# This class is not exposed at runtime. It calls itself sys.int_info.
@final
@type_check_only
class _int_info(structseq[int], tuple[int, int, int, int]):
    """sys.int_info

    A named tuple that holds information about Python's
    internal representation of integers.  The attributes are read only.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("bits_per_digit", "sizeof_digit", "default_max_str_digits", "str_digits_check_threshold")

    @property
    def bits_per_digit(self) -> int:
        """size of a digit in bits"""

    @property
    def sizeof_digit(self) -> int:
        """size in bytes of the C type used to represent a digit"""

    @property
    def default_max_str_digits(self) -> int:
        """maximum string conversion digits limitation"""

    @property
    def str_digits_check_threshold(self) -> int:
        """minimum positive value for int_max_str_digits"""

_ThreadInfoName: TypeAlias = Literal["nt", "pthread", "pthread-stubs", "solaris"]
_ThreadInfoLock: TypeAlias = Literal["semaphore", "mutex+cond"] | None

# This class is not exposed at runtime. It calls itself sys.thread_info.
@final
@type_check_only
class _thread_info(_UninstantiableStructseq, tuple[_ThreadInfoName, _ThreadInfoLock, str | None]):
    """sys.thread_info

    A named tuple holding information about the thread implementation.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("name", "lock", "version")

    @property
    def name(self) -> _ThreadInfoName:
        """name of the thread implementation"""

    @property
    def lock(self) -> _ThreadInfoLock:
        """name of the lock implementation"""

    @property
    def version(self) -> str | None:
        """name and version of the thread library"""

thread_info: _thread_info
_ReleaseLevel: TypeAlias = Literal["alpha", "beta", "candidate", "final"]

# This class is not exposed at runtime. It calls itself sys.version_info.
@final
@type_check_only
class _version_info(_UninstantiableStructseq, tuple[int, int, int, _ReleaseLevel, int]):
    """sys.version_info

    Version information as a named tuple.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("major", "minor", "micro", "releaselevel", "serial")

    @property
    def major(self) -> int:
        """Major release number"""

    @property
    def minor(self) -> int:
        """Minor release number"""

    @property
    def micro(self) -> int:
        """Patch release number"""

    @property
    def releaselevel(self) -> _ReleaseLevel:
        """'alpha', 'beta', 'candidate', or 'final'"""

    @property
    def serial(self) -> int:
        """Serial release number"""

version_info: _version_info

def call_tracing(func: Callable[..., _T], args: Any, /) -> _T:
    """Call func(*args), while tracing is enabled.

    The tracing state is saved, and restored afterwards.  This is intended
    to be called from a debugger from a checkpoint, to recursively debug
    some other code.
    """

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13. Use `_clear_internal_caches()` instead.")
    def _clear_type_cache() -> None:
        """Clear the internal type lookup cache."""

else:
    def _clear_type_cache() -> None:
        """Clear the internal type lookup cache."""

def _current_frames() -> dict[int, FrameType]:
    """Return a dict mapping each thread's thread id to its current stack frame.

    This function should be used for specialized purposes only.
    """

def _getframe(depth: int = 0, /) -> FrameType:
    """Return a frame object from the call stack.

    If optional integer depth is given, return the frame object that many
    calls below the top of the stack.  If that is deeper than the call
    stack, ValueError is raised.  The default for depth is zero, returning
    the frame at the top of the call stack.

    This function should be used for internal and specialized purposes
    only.
    """

# documented -- see https://docs.python.org/3/library/sys.html#sys._current_exceptions
if sys.version_info >= (3, 12):
    def _current_exceptions() -> dict[int, BaseException | None]:
        """Return a dict mapping each thread's identifier to its current raised exception.

        This function should be used for specialized purposes only.
        """

else:
    def _current_exceptions() -> dict[int, OptExcInfo]:
        """Return a dict mapping each thread's identifier to its current raised exception.

        This function should be used for specialized purposes only.
        """

if sys.version_info >= (3, 12):
    def _getframemodulename(depth: int = 0) -> str | None:
        """Return the name of the module for a calling frame.

        The default depth returns the module containing the call to this API.
        A more typical use in a library will pass a depth of 1 to get the user's
        module rather than the library module.

        If no frame, module, or name can be found, returns None.
        """

def _debugmallocstats() -> None:
    """Print summary info to stderr about the state of pymalloc's structures.

    In Py_DEBUG mode, also perform some expensive internal consistency
    checks.
    """

def __displayhook__(object: object, /) -> None:
    """Print an object to sys.stdout and also save it in builtins._"""

def __excepthook__(exctype: type[BaseException], value: BaseException, traceback: TracebackType | None, /) -> None:
    """Handle an exception by displaying it with a traceback on sys.stderr."""

def exc_info() -> OptExcInfo:
    """Return current exception information: (type, value, traceback).

    Return information about the most recent exception caught by an except
    clause in the current stack frame or in an older stack frame.
    """

if sys.version_info >= (3, 11):
    def exception() -> BaseException | None:
        """Return the current exception.

        Return the most recent exception caught by an except clause
        in the current stack frame or in an older stack frame, or None
        if no such exception exists.
        """

def exit(status: _ExitCode = None, /) -> NoReturn:
    """Exit the interpreter by raising SystemExit(status).

    If the status is omitted or None, it defaults to zero (i.e., success).
    If the status is an integer, it will be used as the system exit status.
    If it is another kind of object, it will be printed and the system
    exit status will be one (i.e., failure).
    """

if sys.platform == "android":  # noqa: Y008
    def getandroidapilevel() -> int: ...

def getallocatedblocks() -> int:
    """Return the number of memory blocks currently allocated."""

def getdefaultencoding() -> Literal["utf-8"]:
    """Return the current default encoding used by the Unicode implementation."""

if sys.platform != "win32":
    def getdlopenflags() -> int:
        """Return the current value of the flags that are used for dlopen calls.

        The flag constants are defined in the os module.
        """

def getfilesystemencoding() -> LiteralString:
    """Return the encoding used to convert Unicode filenames to OS filenames."""

def getfilesystemencodeerrors() -> LiteralString:
    """Return the error mode used Unicode to OS filename conversion."""

def getrefcount(object: Any, /) -> int:
    """Return the reference count of object.

    The count returned is generally one higher than you might expect,
    because it includes the (temporary) reference as an argument to
    getrefcount().
    """

def getrecursionlimit() -> int:
    """Return the current value of the recursion limit.

    The recursion limit is the maximum depth of the Python interpreter
    stack.  This limit prevents infinite recursion from causing an overflow
    of the C stack and crashing Python.
    """

def getsizeof(obj: object, default: int = ...) -> int:
    """getsizeof(object [, default]) -> int

    Return the size of object in bytes.
    """

def getswitchinterval() -> float:
    """Return the current thread switch interval; see sys.setswitchinterval()."""

def getprofile() -> ProfileFunction | None:
    """Return the profiling function set with sys.setprofile.

    See the profiler chapter in the library manual.
    """

def setprofile(function: ProfileFunction | None, /) -> None:
    """Set the profiling function.

    It will be called on each function call and return.  See the profiler
    chapter in the library manual.
    """

def gettrace() -> TraceFunction | None:
    """Return the global debug tracing function set with sys.settrace.

    See the debugger chapter in the library manual.
    """

def settrace(function: TraceFunction | None, /) -> None:
    """Set the global debug tracing function.

    It will be called on each function call.  See the debugger chapter
    in the library manual.
    """

if sys.platform == "win32":
    # A tuple of length 5, even though it has more than 5 attributes.
    @final
    @type_check_only
    class _WinVersion(_UninstantiableStructseq, tuple[int, int, int, int, str]):
        @property
        def major(self) -> int: ...
        @property
        def minor(self) -> int: ...
        @property
        def build(self) -> int: ...
        @property
        def platform(self) -> int: ...
        @property
        def service_pack(self) -> str: ...
        @property
        def service_pack_minor(self) -> int: ...
        @property
        def service_pack_major(self) -> int: ...
        @property
        def suite_mask(self) -> int: ...
        @property
        def product_type(self) -> int: ...
        @property
        def platform_version(self) -> tuple[int, int, int]: ...

    def getwindowsversion() -> _WinVersion:
        """Return info about the running version of Windows as a named tuple.

        The members are named: major, minor, build, platform, service_pack,
        service_pack_major, service_pack_minor, suite_mask, product_type and
        platform_version. For backward compatibility, only the first 5 items
        are available by indexing. All elements are numbers, except
        service_pack and platform_type which are strings, and platform_version
        which is a 3-tuple. Platform is always 2. Product_type may be 1 for a
        workstation, 2 for a domain controller, 3 for a server.
        Platform_version is a 3-tuple containing a version number that is
        intended for identifying the OS rather than feature detection.
        """

@overload
def intern(string: LiteralString, /) -> LiteralString:
    """``Intern'' the given string.

    This enters the string in the (global) table of interned strings whose
    purpose is to speed up dictionary lookups. Return the string itself or
    the previously interned string object with the same value.
    """

@overload
def intern(string: str, /) -> str: ...  # type: ignore[misc]

__interactivehook__: Callable[[], object]

if sys.version_info >= (3, 13):
    def _is_gil_enabled() -> bool:
        """Return True if the GIL is currently enabled and False otherwise."""

    def _clear_internal_caches() -> None:
        """Clear all internal performance-related caches."""

    def _is_interned(string: str, /) -> bool:
        """Return True if the given string is "interned"."""

def is_finalizing() -> bool:
    """Return True if Python is exiting."""

def breakpointhook(*args: Any, **kwargs: Any) -> Any:
    """This hook function is called by built-in breakpoint()."""

__breakpointhook__ = breakpointhook  # Contains the original value of breakpointhook

if sys.platform != "win32":
    def setdlopenflags(flags: int, /) -> None:
        """Set the flags used by the interpreter for dlopen calls.

        This is used, for example, when the interpreter loads extension
        modules. Among other things, this will enable a lazy resolving of
        symbols when importing a module, if called as sys.setdlopenflags(0).
        To share symbols across extension modules, call as
        sys.setdlopenflags(os.RTLD_GLOBAL).  Symbolic names for the flag
        modules can be found in the os module (RTLD_xxx constants, e.g.
        os.RTLD_LAZY).
        """

def setrecursionlimit(limit: int, /) -> None:
    """Set the maximum depth of the Python interpreter stack to n.

    This limit prevents infinite recursion from causing an overflow of the C
    stack and crashing Python.  The highest possible limit is platform-
    dependent.
    """

def setswitchinterval(interval: float, /) -> None:
    """Set the ideal thread switching delay inside the Python interpreter.

    The actual frequency of switching threads can be lower if the
    interpreter executes long sequences of uninterruptible code
    (this is implementation-specific and workload-dependent).

    The parameter must represent the desired switching delay in seconds
    A typical value is 0.005 (5 milliseconds).
    """

def gettotalrefcount() -> int: ...  # Debug builds only

# Doesn't exist at runtime, but exported in the stubs so pytest etc. can annotate their code more easily.
@type_check_only
class UnraisableHookArgs(Protocol):
    exc_type: type[BaseException]
    exc_value: BaseException | None
    exc_traceback: TracebackType | None
    err_msg: str | None
    object: _object

unraisablehook: Callable[[UnraisableHookArgs], Any]

def __unraisablehook__(unraisable: UnraisableHookArgs, /) -> Any:
    """Handle an unraisable exception.

    The unraisable argument has the following attributes:

    * exc_type: Exception type.
    * exc_value: Exception value, can be None.
    * exc_traceback: Exception traceback, can be None.
    * err_msg: Error message, can be None.
    * object: Object causing the exception, can be None.
    """

def addaudithook(hook: Callable[[str, tuple[Any, ...]], Any]) -> None:
    """Adds a new audit hook callback."""

def audit(event: str, /, *args: Any) -> None:
    """Passes the event to any audit hooks that are attached."""

_AsyncgenHook: TypeAlias = Callable[[AsyncGenerator[Any, Any]], None] | None

# This class is not exposed at runtime. It calls itself builtins.asyncgen_hooks.
@final
@type_check_only
class _asyncgen_hooks(structseq[_AsyncgenHook], tuple[_AsyncgenHook, _AsyncgenHook]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("firstiter", "finalizer")

    @property
    def firstiter(self) -> _AsyncgenHook: ...
    @property
    def finalizer(self) -> _AsyncgenHook: ...

def get_asyncgen_hooks() -> _asyncgen_hooks:
    """Return the installed asynchronous generators hooks.

    This returns a namedtuple of the form (firstiter, finalizer).
    """

def set_asyncgen_hooks(firstiter: _AsyncgenHook = ..., finalizer: _AsyncgenHook = ...) -> None:
    """set_asyncgen_hooks([firstiter] [, finalizer])

    Set a finalizer for async generators objects.
    """

if sys.platform == "win32":
    if sys.version_info >= (3, 13):
        @deprecated(
            "Deprecated since Python 3.13; will be removed in Python 3.16. "
            "Use the `PYTHONLEGACYWINDOWSFSENCODING` environment variable instead."
        )
        def _enablelegacywindowsfsencoding() -> None:
            """Changes the default filesystem encoding to mbcs:replace.

            This is done for consistency with earlier versions of Python. See PEP
            529 for more information.

            This is equivalent to defining the PYTHONLEGACYWINDOWSFSENCODING
            environment variable before launching Python.
            """
    else:
        def _enablelegacywindowsfsencoding() -> None:
            """Changes the default filesystem encoding to mbcs:replace.

            This is done for consistency with earlier versions of Python. See PEP
            529 for more information.

            This is equivalent to defining the PYTHONLEGACYWINDOWSFSENCODING
            environment variable before launching Python.
            """

def get_coroutine_origin_tracking_depth() -> int:
    """Check status of origin tracking for coroutine objects in this thread."""

def set_coroutine_origin_tracking_depth(depth: int) -> None:
    """Enable or disable origin tracking for coroutine objects in this thread.

    Coroutine objects will track 'depth' frames of traceback information
    about where they came from, available in their cr_origin attribute.

    Set a depth of 0 to disable.
    """

# The following two functions were added in 3.11.0, 3.10.7, and 3.9.14,
# as part of the response to CVE-2020-10735
def set_int_max_str_digits(maxdigits: int) -> None:
    """Set the maximum string digits limit for non-binary int<->str conversions."""

def get_int_max_str_digits() -> int:
    """Return the maximum string digits limit for non-binary int<->str conversions."""

if sys.version_info >= (3, 12):
    if sys.version_info >= (3, 13):
        def getunicodeinternedsize(*, _only_immortal: bool = False) -> int:
            """Return the number of elements of the unicode interned dictionary"""
    else:
        def getunicodeinternedsize() -> int:
            """Return the number of elements of the unicode interned dictionary"""

    def deactivate_stack_trampoline() -> None:
        """Deactivate the current stack profiler trampoline backend.

        If no stack profiler is activated, this function has no effect.
        """

    def is_stack_trampoline_active() -> bool:
        """Return *True* if a stack profiler trampoline is active."""
    # It always exists, but raises on non-linux platforms:
    if sys.platform == "linux":
        def activate_stack_trampoline(backend: str, /) -> None:
            """Activate stack profiler trampoline *backend*."""
    else:
        def activate_stack_trampoline(backend: str, /) -> NoReturn:
            """Activate stack profiler trampoline *backend*."""
    from . import _monitoring

    monitoring = _monitoring

if sys.version_info >= (3, 14):
    def is_remote_debug_enabled() -> bool:
        """Return True if remote debugging is enabled, False otherwise."""

    def remote_exec(pid: int, script: StrOrBytesPath) -> None:
        """Executes a file containing Python code in a given remote Python process.

        This function returns immediately, and the code will be executed by the
        target process's main thread at the next available opportunity, similarly
        to how signals are handled. There is no interface to determine when the
        code has been executed. The caller is responsible for making sure that
        the file still exists whenever the remote process tries to read it and that
        it hasn't been overwritten.

        The remote process must be running a CPython interpreter of the same major
        and minor version as the local process. If either the local or remote
        interpreter is pre-release (alpha, beta, or release candidate) then the
        local and remote interpreters must be the same exact version.

        Args:
             pid (int): The process ID of the target Python process.
             script (str|bytes): The path to a file containing
                 the Python code to be executed.
        """

    def _is_immortal(op: object, /) -> bool:
        """Return True if the given object is "immortal" per PEP 683.

        This function should be used for specialized purposes only.
        """
