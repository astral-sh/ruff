import sys
from typing import TextIO

__all__ = ["getpass", "getuser", "GetPassWarning"]

if sys.version_info >= (3, 14):
    def getpass(prompt: str = "Password: ", stream: TextIO | None = None, *, echo_char: str | None = None) -> str: ...

else:
    def getpass(prompt: str = "Password: ", stream: TextIO | None = None) -> str: ...

def getuser() -> str: ...

class GetPassWarning(UserWarning): ...
