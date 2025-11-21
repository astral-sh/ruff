"""This module provides primitive operations to manage Python interpreters.
The 'interpreters' module provides a more convenient interface.
"""

import types
from collections.abc import Callable
from typing import Any, Final, Literal, SupportsIndex, TypeVar, overload
from typing_extensions import TypeAlias, disjoint_base

_R = TypeVar("_R")

_Configs: TypeAlias = Literal["default", "isolated", "legacy", "empty", ""]
_SharedDict: TypeAlias = dict[str, Any]  # many objects can be shared

class InterpreterError(Exception):
    """A cross-interpreter operation failed"""

class InterpreterNotFoundError(InterpreterError):
    """An interpreter was not found"""

class NotShareableError(ValueError): ...

@disjoint_base
class CrossInterpreterBufferView:
    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

def new_config(name: _Configs = "isolated", /, **overides: object) -> types.SimpleNamespace:
    """new_config(name='isolated', /, **overrides) -> type.SimpleNamespace

    Return a representation of a new PyInterpreterConfig.

    The name determines the initial values of the config.  Supported named
    configs are: default, isolated, legacy, and empty.

    Any keyword arguments are set on the corresponding config fields,
    overriding the initial values.
    """

def create(config: types.SimpleNamespace | _Configs | None = "isolated", *, reqrefs: bool = False) -> int:
    """create([config], *, reqrefs=False) -> ID

    Create a new interpreter and return a unique generated ID.

    The caller is responsible for destroying the interpreter before exiting,
    typically by using _interpreters.destroy().  This can be managed
    automatically by passing "reqrefs=True" and then using _incref() and
    _decref() appropriately.

    "config" must be a valid interpreter config or the name of a
    predefined config ("isolated" or "legacy").  The default
    is "isolated".
    """

def destroy(id: SupportsIndex, *, restrict: bool = False) -> None:
    """destroy(id, *, restrict=False)

    Destroy the identified interpreter.

    Attempting to destroy the current interpreter raises InterpreterError.
    So does an unrecognized ID.
    """

def list_all(*, require_ready: bool = False) -> list[tuple[int, _Whence]]:
    """list_all() -> [(ID, whence)]

    Return a list containing the ID of every existing interpreter.
    """

def get_current() -> tuple[int, _Whence]:
    """get_current() -> (ID, whence)

    Return the ID of current interpreter.
    """

def get_main() -> tuple[int, _Whence]:
    """get_main() -> (ID, whence)

    Return the ID of main interpreter.
    """

def is_running(id: SupportsIndex, *, restrict: bool = False) -> bool:
    """is_running(id, *, restrict=False) -> bool

    Return whether or not the identified interpreter is running.
    """

def get_config(id: SupportsIndex, *, restrict: bool = False) -> types.SimpleNamespace:
    """get_config(id, *, restrict=False) -> types.SimpleNamespace

    Return a representation of the config used to initialize the interpreter.
    """

def whence(id: SupportsIndex) -> _Whence:
    """whence(id) -> int

    Return an identifier for where the interpreter was created.
    """

def exec(
    id: SupportsIndex, code: str | types.CodeType | Callable[[], object], shared: _SharedDict = {}, *, restrict: bool = False
) -> None | types.SimpleNamespace:
    """exec(id, code, shared=None, *, restrict=False)

    Execute the provided code in the identified interpreter.
    This is equivalent to running the builtin exec() under the target
    interpreter, using the __dict__ of its __main__ module as both
    globals and locals.

    "code" may be a string containing the text of a Python script.

    Functions (and code objects) are also supported, with some restrictions.
    The code/function must not take any arguments or be a closure
    (i.e. have cell vars).  Methods and other callables are not supported.

    If a function is provided, its code object is used and all its state
    is ignored, including its __globals__ dict.
    """

def call(
    id: SupportsIndex,
    callable: Callable[..., _R],
    args: tuple[Any, ...] = (),
    kwargs: dict[str, Any] = {},
    *,
    preserve_exc: bool = False,
    restrict: bool = False,
) -> tuple[_R, types.SimpleNamespace]:
    """call(id, callable, args=None, kwargs=None, *, restrict=False)

    Call the provided object in the identified interpreter.
    Pass the given args and kwargs, if possible.
    """

def run_string(
    id: SupportsIndex, script: str | types.CodeType | Callable[[], object], shared: _SharedDict = {}, *, restrict: bool = False
) -> None:
    """run_string(id, script, shared=None, *, restrict=False)

    Execute the provided string in the identified interpreter.

    (See _interpreters.exec().
    """

def run_func(
    id: SupportsIndex, func: types.CodeType | Callable[[], object], shared: _SharedDict = {}, *, restrict: bool = False
) -> None:
    """run_func(id, func, shared=None, *, restrict=False)

    Execute the body of the provided function in the identified interpreter.
    Code objects are also supported.  In both cases, closures and args
    are not supported.  Methods and other callables are not supported either.

    (See _interpreters.exec().
    """

def set___main___attrs(id: SupportsIndex, updates: _SharedDict, *, restrict: bool = False) -> None:
    """set___main___attrs(id, ns, *, restrict=False)

    Bind the given attributes in the interpreter's __main__ module.
    """

def incref(id: SupportsIndex, *, implieslink: bool = False, restrict: bool = False) -> None: ...
def decref(id: SupportsIndex, *, restrict: bool = False) -> None: ...
def is_shareable(obj: object) -> bool:
    """is_shareable(obj) -> bool

    Return True if the object's data may be shared between interpreters and
    False otherwise.
    """

@overload
def capture_exception(exc: BaseException) -> types.SimpleNamespace:
    """capture_exception(exc=None) -> types.SimpleNamespace

    Return a snapshot of an exception.  If "exc" is None
    then the current exception, if any, is used (but not cleared).

    The returned snapshot is the same as what _interpreters.exec() returns.
    """

@overload
def capture_exception(exc: None = None) -> types.SimpleNamespace | None: ...

_Whence: TypeAlias = Literal[0, 1, 2, 3, 4, 5]
WHENCE_UNKNOWN: Final = 0
WHENCE_RUNTIME: Final = 1
WHENCE_LEGACY_CAPI: Final = 2
WHENCE_CAPI: Final = 3
WHENCE_XI: Final = 4
WHENCE_STDLIB: Final = 5
