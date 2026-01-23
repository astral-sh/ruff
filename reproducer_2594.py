"""
Minimal reproducer for https://github.com/astral-sh/ty/issues/2594

False positive: "Too many positional arguments to bound method `__init__`"
when using a class as decorator without parentheses, where `__new__`
intercepts the function argument and returns a non-instance.

This pattern is used by torch.no_grad and similar decorators.
"""

from typing import Callable, TypeVar

F = TypeVar("F", bound=Callable[..., object])


class NoParamDecorator:
    """
    A decorator that can be used with or without parentheses:
    - @NoParamDecorator
    - @NoParamDecorator()
    """

    def __new__(cls, func: F | None = None) -> "NoParamDecorator | F":
        # When used as @NoParamDecorator (without parens), func is passed directly.
        # We return the function itself - __init__ is NEVER called in this case.
        if func is not None:
            return func  # Returns the function, not an instance

        # When used as @NoParamDecorator(), return a new instance.
        # __init__ will be called on this instance.
        return super().__new__(cls)

    def __init__(self) -> None:
        # Only takes self - this is only called when __new__ returns an instance
        pass

    def __call__(self, func: F) -> F:
        return func


# ty error: "Too many positional arguments to bound method `__init__`: expected 0, got 1"
# This is a false positive because __new__ returns `func` directly, so __init__ is never called.
@NoParamDecorator
def my_function(x: int) -> int:
    return x + 1


# This works fine - __new__ returns an instance, __init__() is called, then __call__(func)
@NoParamDecorator()
def my_other_function(x: int) -> int:
    return x + 1
