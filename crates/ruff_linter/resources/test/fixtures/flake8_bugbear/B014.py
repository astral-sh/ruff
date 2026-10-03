"""
Exact duplicate exceptions are rejected in all modes.
Redundant built-in exception subclasses are rejected only in preview.
"""

import binascii
import re

try:
    pass
except (Exception, TypeError):
    # TypeError is a subclass of Exception, so it doesn't add anything
    pass

try:
    pass
except (OSError, OSError) as err:
    # Duplicate exception types are useless
    pass


class MyError(Exception):
    pass


try:
    pass
except (MyError, MyError):
    # Detect duplicate non-builtin errors
    pass


try:
    pass
except (MyError, Exception) as e:
    # Don't assume that we're all subclasses of Exception
    pass


try:
    pass
except (MyError, BaseException) as e:
    # Custom exception inheritance is not inferred.
    pass


try:
    pass
except (re.error, re.error):
    # Duplicate exception types as attributes
    pass


try:
    pass
except (IOError, EnvironmentError, OSError):
    # Detect if a primary exception and any its aliases are present.
    #
    # Since Python 3.3, IOError, EnvironmentError, WindowsError, mmap.error,
    # socket.error and select.error are aliases of OSError. See PEP 3151 for
    # more info.
    pass


try:
    pass
except (MyException, NotImplemented):
    # NotImplemented is not an exception, let's not crash on it.
    pass


try:
    pass
except (ValueError, binascii.Error):
    # binascii.Error is a subclass of ValueError.
    pass


# Regression test for: https://github.com/astral-sh/ruff/issues/6412
try:
    pass
except (ValueError, ValueError, TypeError):
    pass


# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1739801758
try:
    pas
except(re.error, re.error):
    p


try:
    pass
except (
    ValueError,
    ValueError,
    # text
    TypeError,
):
    pass


# Built-in subclasses are redundant regardless of tuple order.
try:
    pass
except (OSError, TimeoutError):
    pass

try:
    pass
except (TimeoutError, OSError):
    pass

try:
    pass
except (ValueError, UnicodeDecodeError):
    pass

try:
    pass
except (UnicodeDecodeError, ValueError):
    pass

# Remove intermediate bases as well as their subclasses.
try:
    pass
except (UnicodeDecodeError, UnicodeError, ValueError):
    pass

try:
    pass
except (OSError, TimeoutError, FileNotFoundError):
    pass

# Preserve the order of unrelated survivors.
try:
    pass
except (TypeError, TimeoutError, OSError, ValueError):
    pass

try:
    pass
except (TimeoutError, FileNotFoundError):
    pass

try:
    pass
except (Exception, ValueError):
    pass

try:
    pass
except (Exception, KeyboardInterrupt):
    pass

try:
    pass
except (KeyboardInterrupt, BaseException):
    pass

# Exact duplicates and hierarchy redundancy can occur together.
try:
    pass
except (TimeoutError, OSError, TimeoutError, OSError):
    pass

# More than one intermediate base is supported.
try:
    pass
except (TabError, SyntaxError, IndentationError):
    pass

try:
    pass
except (BrokenPipeError, OSError):
    pass

try:
    pass
except (UserWarning, Warning):
    pass


def shadowed_child():
    TimeoutError = MyError
    try:
        pass
    except (OSError, TimeoutError):
        pass


def shadowed_parent(OSError):
    try:
        pass
    except (OSError, TimeoutError):
        pass


def shadowed_child_class():
    class TimeoutError(Exception):
        pass

    try:
        pass
    except (OSError, TimeoutError):
        pass


def rebound_after_handler():
    from builtins import TimeoutError

    try:
        pass
    except (OSError, TimeoutError):
        TimeoutError = MyError


import builtins
import builtins as b
from builtins import OSError as OS, TimeoutError as Timeout

try:
    pass
except (builtins.TimeoutError, builtins.OSError):
    pass

try:
    pass
except (b.OSError, b.TimeoutError):
    pass

try:
    pass
except (Timeout, OS):
    pass


def rebound_builtin_import():
    from builtins import TimeoutError as Timeout

    Timeout = MyError
    try:
        pass
    except (OS, Timeout):
        pass


def shadowed_builtin_module(builtins):
    try:
        pass
    except (builtins.OSError, TimeoutError):
        pass


# Assignment aliases, equivalence aliases, and imported exception hierarchies
# are outside the built-in subclass check.
Alias = TimeoutError
try:
    pass
except (OSError, Alias):
    pass

try:
    pass
except (OSError, IOError, EnvironmentError):
    pass

from urllib.error import URLError

try:
    pass
except (OSError, URLError):
    pass

# Hierarchy checks do not reinterpret invalid non-exception names.
try:
    pass
except (Exception, NotImplemented):
    pass

# A hierarchy fix must not drop an expression the existing fixer cannot retain.
try:
    pass
except (OSError, TimeoutError, get_exception()):
    pass

# Comments within the replaced tuple make the fix unsafe.
try:
    pass
except (
    TimeoutError,  # A narrower exception.
    OSError,
):
    pass

# Comments outside the tuple survive the fix.
try:
    pass
except (TimeoutError, OSError):  # Keep this comment.
    pass

# Ordinary subclasses also match the same exception-group members in except*.
try:
    pass
except* (TimeoutError, OSError):
    pass

# Exception-group classes are excluded from hierarchy checks, including in
# except*, where using these classes as matching types raises TypeError.
try:
    pass
except (Exception, ExceptionGroup):
    pass

try:
    pass
except (BaseExceptionGroup, ExceptionGroup):
    pass

try:
    pass
except* (Exception, ExceptionGroup):
    pass
