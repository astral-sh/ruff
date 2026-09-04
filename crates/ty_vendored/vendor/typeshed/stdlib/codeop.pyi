import sys
from types import CodeType

__all__ = ["compile_command", "Compile", "CommandCompiler"]

if sys.version_info >= (3, 14):
    def compile_command(source: str, filename: str = "<input>", symbol: str = "single", flags: int = 0) -> CodeType | None: ...

else:
    def compile_command(source: str, filename: str = "<input>", symbol: str = "single") -> CodeType | None: ...

class Compile:
    flags: int
    if sys.version_info >= (3, 13):
        def __call__(self, source: str, filename: str, symbol: str, flags: int = 0) -> CodeType: ...
    else:
        def __call__(self, source: str, filename: str, symbol: str) -> CodeType: ...

class CommandCompiler:
    compiler: Compile
    def __call__(self, source: str, filename: str = "<input>", symbol: str = "single") -> CodeType | None: ...
