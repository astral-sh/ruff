import sys

def find_library(name: str) -> str | None: ...

if sys.platform == "win32":
    def find_msvcrt() -> str | None: ...

if sys.version_info >= (3, 14):
    def dllist() -> list[str]:
        """Return a list of loaded shared libraries in the current process.
        """

def test() -> None: ...
