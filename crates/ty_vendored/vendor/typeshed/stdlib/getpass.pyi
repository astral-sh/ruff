"""Utilities to get a password and/or the current user name.

getpass(prompt[, stream[, echo_char]]) - Prompt for a password, with echo
turned off and optional keyboard feedback.
getuser() - Get the user name from the environment or password database.

GetPassWarning - This UserWarning is issued when getpass() cannot prevent
                 echoing of the password contents while reading.

On Windows, the msvcrt module will be used.

"""

import sys
from typing import TextIO

__all__ = ["getpass", "getuser", "GetPassWarning"]

if sys.version_info >= (3, 14):
    def getpass(prompt: str = "Password: ", stream: TextIO | None = None, *, echo_char: str | None = None) -> str:
        """Prompt for a password, with echo turned off.

        Args:
          prompt: Written on stream to ask for the input.  Default: 'Password: '
          stream: A writable file object to display the prompt.  Defaults to
                  the tty.  If no tty is available defaults to sys.stderr.
          echo_char: A single ASCII character to mask input (e.g., '*').
                  If None, input is hidden.
        Returns:
          The seKr3t input.
        Raises:
          EOFError: If our input tty or stdin was closed.
          GetPassWarning: When we were unable to turn echo off on the input.

        Always restores terminal settings before returning.
        """

else:
    def getpass(prompt: str = "Password: ", stream: TextIO | None = None) -> str:
        """Prompt for a password, with echo turned off.

        Args:
          prompt: Written on stream to ask for the input.  Default: 'Password: '
          stream: A writable file object to display the prompt.  Defaults to
                  the tty.  If no tty is available defaults to sys.stderr.
        Returns:
          The seKr3t input.
        Raises:
          EOFError: If our input tty or stdin was closed.
          GetPassWarning: When we were unable to turn echo off on the input.

        Always restores terminal settings before returning.
        """

def getuser() -> str:
    """Get the username from the environment or password database.

    First try various environment variables, then the password
    database.  This works on Windows as long as USERNAME is set.
    Any failure to find a username raises OSError.

    .. versionchanged:: 3.13
        Previously, various exceptions beyond just :exc:`OSError`
        were raised.
    """

class GetPassWarning(UserWarning): ...
