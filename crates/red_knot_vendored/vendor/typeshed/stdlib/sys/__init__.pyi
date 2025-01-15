import sys
from _typeshed import MaybeNone, OptExcInfo, ProfileFunction, TraceFunction, structseq
from _typeshed.importlib import MetaPathFinderProtocol, PathEntryFinderProtocol
from builtins import object as _object
from collections.abc import AsyncGenerator, Callable, Sequence
from io import TextIOWrapper
from types import FrameType, ModuleType, TracebackType
from typing import Any, Final, Literal, NoReturn, Protocol, TextIO, TypeVar, final, type_check_only
from typing_extensions import LiteralString, TypeAlias

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
if sys.version_info >= (3, 9):
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
# 3.8 and 3.9 are 15 or 16-tuple
# 3.10 is 16 or 17-tuple
# 3.11+ is an 18-tuple.
@final
@type_check_only
class _flags(_UninstantiableStructseq, tuple[int, ...]):
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
    def debug(self) -> int: ...
    @property
    def inspect(self) -> int: ...
    @property
    def interactive(self) -> int: ...
    @property
    def optimize(self) -> int: ...
    @property
    def dont_write_bytecode(self) -> int: ...
    @property
    def no_user_site(self) -> int: ...
    @property
    def no_site(self) -> int: ...
    @property
    def ignore_environment(self) -> int: ...
    @property
    def verbose(self) -> int: ...
    @property
    def bytes_warning(self) -> int: ...
    @property
    def quiet(self) -> int: ...
    @property
    def hash_randomization(self) -> int: ...
    @property
    def isolated(self) -> int: ...
    @property
    def dev_mode(self) -> bool: ...
    @property
    def utf8_mode(self) -> int: ...
    if sys.version_info >= (3, 10):
        @property
        def warn_default_encoding(self) -> int: ...
    if sys.version_info >= (3, 11):
        @property
        def safe_path(self) -> bool: ...
    # Whether or not this exists on lower versions of Python
    # may depend on which patch release you're using
    # (it was backported to all Python versions on 3.8+ as a security fix)
    # Added in: 3.8.14, 3.9.14, 3.10.7
    # and present in all versions of 3.11 and later.
    @property
    def int_max_str_digits(self) -> int: ...

float_info: _float_info

# This class is not exposed at runtime. It calls itself sys.float_info.
@final
@type_check_only
class _float_info(structseq[float], tuple[float, int, int, float, int, int, int, int, float, int, int]):
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
    def max(self) -> float: ...  # DBL_MAX
    @property
    def max_exp(self) -> int: ...  # DBL_MAX_EXP
    @property
    def max_10_exp(self) -> int: ...  # DBL_MAX_10_EXP
    @property
    def min(self) -> float: ...  # DBL_MIN
    @property
    def min_exp(self) -> int: ...  # DBL_MIN_EXP
    @property
    def min_10_exp(self) -> int: ...  # DBL_MIN_10_EXP
    @property
    def dig(self) -> int: ...  # DBL_DIG
    @property
    def mant_dig(self) -> int: ...  # DBL_MANT_DIG
    @property
    def epsilon(self) -> float: ...  # DBL_EPSILON
    @property
    def radix(self) -> int: ...  # FLT_RADIX
    @property
    def rounds(self) -> int: ...  # FLT_ROUNDS

hash_info: _hash_info

# This class is not exposed at runtime. It calls itself sys.hash_info.
@final
@type_check_only
class _hash_info(structseq[Any | int], tuple[int, int, int, int, int, str, int, int, int]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("width", "modulus", "inf", "nan", "imag", "algorithm", "hash_bits", "seed_bits", "cutoff")

    @property
    def width(self) -> int: ...
    @property
    def modulus(self) -> int: ...
    @property
    def inf(self) -> int: ...
    @property
    def nan(self) -> int: ...
    @property
    def imag(self) -> int: ...
    @property
    def algorithm(self) -> str: ...
    @property
    def hash_bits(self) -> int: ...
    @property
    def seed_bits(self) -> int: ...
    @property
    def cutoff(self) -> int: ...  # undocumented

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
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("bits_per_digit", "sizeof_digit", "default_max_str_digits", "str_digits_check_threshold")

    @property
    def bits_per_digit(self) -> int: ...
    @property
    def sizeof_digit(self) -> int: ...
    @property
    def default_max_str_digits(self) -> int: ...
    @property
    def str_digits_check_threshold(self) -> int: ...

_ThreadInfoName: TypeAlias = Literal["nt", "pthread", "pthread-stubs", "solaris"]
_ThreadInfoLock: TypeAlias = Literal["semaphore", "mutex+cond"] | None

# This class is not exposed at runtime. It calls itself sys.thread_info.
@final
@type_check_only
class _thread_info(_UninstantiableStructseq, tuple[_ThreadInfoName, _ThreadInfoLock, str | None]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("name", "lock", "version")

    @property
    def name(self) -> _ThreadInfoName: ...
    @property
    def lock(self) -> _ThreadInfoLock: ...
    @property
    def version(self) -> str | None: ...

thread_info: _thread_info
_ReleaseLevel: TypeAlias = Literal["alpha", "beta", "candidate", "final"]

# This class is not exposed at runtime. It calls itself sys.version_info.
@final
@type_check_only
class _version_info(_UninstantiableStructseq, tuple[int, int, int, _ReleaseLevel, int]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("major", "minor", "micro", "releaselevel", "serial")

    @property
    def major(self) -> int: ...
    @property
    def minor(self) -> int: ...
    @property
    def micro(self) -> int: ...
    @property
    def releaselevel(self) -> _ReleaseLevel: ...
    @property
    def serial(self) -> int: ...

version_info: _version_info

def call_tracing(func: Callable[..., _T], args: Any, /) -> _T: ...
def _clear_type_cache() -> None: ...
def _current_frames() -> dict[int, FrameType]: ...
def _getframe(depth: int = 0, /) -> FrameType: ...
def _debugmallocstats() -> None: ...
def __displayhook__(object: object, /) -> None: ...
def __excepthook__(exctype: type[BaseException], value: BaseException, traceback: TracebackType | None, /) -> None: ...
def exc_info() -> OptExcInfo: ...

if sys.version_info >= (3, 11):
    def exception() -> BaseException | None: ...

def exit(status: _ExitCode = None, /) -> NoReturn: ...
def getallocatedblocks() -> int: ...
def getdefaultencoding() -> str: ...

if sys.platform != "win32":
    def getdlopenflags() -> int: ...

def getfilesystemencoding() -> str: ...
def getfilesystemencodeerrors() -> str: ...
def getrefcount(object: Any, /) -> int: ...
def getrecursionlimit() -> int: ...
def getsizeof(obj: object, default: int = ...) -> int: ...
def getswitchinterval() -> float: ...
def getprofile() -> ProfileFunction | None: ...
def setprofile(function: ProfileFunction | None, /) -> None: ...
def gettrace() -> TraceFunction | None: ...
def settrace(function: TraceFunction | None, /) -> None: ...

if sys.platform == "win32":
    # A tuple of length 5, even though it has more than 5 attributes.
    @final
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

    def getwindowsversion() -> _WinVersion: ...

def intern(string: str, /) -> str: ...

if sys.version_info >= (3, 13):
    def _is_gil_enabled() -> bool: ...

def is_finalizing() -> bool: ...
def breakpointhook(*args: Any, **kwargs: Any) -> Any: ...

__breakpointhook__ = breakpointhook  # Contains the original value of breakpointhook

if sys.platform != "win32":
    def setdlopenflags(flags: int, /) -> None: ...

def setrecursionlimit(limit: int, /) -> None: ...
def setswitchinterval(interval: float, /) -> None: ...
def gettotalrefcount() -> int: ...  # Debug builds only

if sys.version_info < (3, 9):
    def getcheckinterval() -> int: ...  # deprecated
    def setcheckinterval(n: int, /) -> None: ...  # deprecated

if sys.version_info < (3, 9):
    # An 11-tuple or None
    def callstats() -> tuple[int, int, int, int, int, int, int, int, int, int, int] | None: ...

# Doesn't exist at runtime, but exported in the stubs so pytest etc. can annotate their code more easily.
@type_check_only
class UnraisableHookArgs(Protocol):
    exc_type: type[BaseException]
    exc_value: BaseException | None
    exc_traceback: TracebackType | None
    err_msg: str | None
    object: _object

unraisablehook: Callable[[UnraisableHookArgs], Any]

def __unraisablehook__(unraisable: UnraisableHookArgs, /) -> Any: ...
def addaudithook(hook: Callable[[str, tuple[Any, ...]], Any]) -> None: ...
def audit(event: str, /, *args: Any) -> None: ...

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

def get_asyncgen_hooks() -> _asyncgen_hooks: ...
def set_asyncgen_hooks(firstiter: _AsyncgenHook = ..., finalizer: _AsyncgenHook = ...) -> None: ...

if sys.platform == "win32":
    def _enablelegacywindowsfsencoding() -> None: ...

def get_coroutine_origin_tracking_depth() -> int: ...
def set_coroutine_origin_tracking_depth(depth: int) -> None: ...

# The following two functions were added in 3.11.0, 3.10.7, 3.9.14, and 3.8.14,
# as part of the response to CVE-2020-10735
def set_int_max_str_digits(maxdigits: int) -> None: ...
def get_int_max_str_digits() -> int: ...

if sys.version_info >= (3, 12):
    if sys.version_info >= (3, 13):
        def getunicodeinternedsize(*, _only_immortal: bool = False) -> int: ...
    else:
        def getunicodeinternedsize() -> int: ...

    def deactivate_stack_trampoline() -> None: ...
    def is_stack_trampoline_active() -> bool: ...
    # It always exists, but raises on non-linux platforms:
    if sys.platform == "linux":
        def activate_stack_trampoline(backend: str, /) -> None: ...
    else:
        def activate_stack_trampoline(backend: str, /) -> NoReturn: ...

    from . import _monitoring

    monitoring = _monitoring
