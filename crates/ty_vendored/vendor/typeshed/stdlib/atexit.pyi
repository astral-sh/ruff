"""allow programmer to define multiple exit functions to be executed
upon normal program termination.

Two public functions, register and unregister, are defined.
"""

from collections.abc import Callable
from typing import TypeVar
from typing_extensions import ParamSpec

_T = TypeVar("_T")
_P = ParamSpec("_P")

def _clear() -> None:
    """Clear the list of previously registered exit functions."""

def _ncallbacks() -> int:
    """Return the number of registered exit functions."""

def _run_exitfuncs() -> None:
    """Run all registered exit functions.

    If a callback raises an exception, it is logged with sys.unraisablehook.
    """

def register(func: Callable[_P, _T], /, *args: _P.args, **kwargs: _P.kwargs) -> Callable[_P, _T]:
    """Register a function to be executed upon normal program termination

    func - function to be called at exit
    args - optional arguments to pass to func
    kwargs - optional keyword arguments to pass to func

    func is returned to facilitate usage as a decorator.
    """

def unregister(func: Callable[..., object], /) -> None:
    """Unregister an exit function which was previously registered using
    atexit.register

        func - function to be unregistered
    """
