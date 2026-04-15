"""distutils._msvccompiler

Contains MSVCCompiler, an implementation of the abstract CCompiler class
for Microsoft Visual Studio 2015.

The module is compatible with VS 2015 and later. You can find legacy support
for older versions in distutils.msvc9compiler and distutils.msvccompiler.
"""

from _typeshed import Incomplete
from distutils.ccompiler import CCompiler
from typing import ClassVar, Final

PLAT_SPEC_TO_RUNTIME: Final[dict[str, str]]
PLAT_TO_VCVARS: Final[dict[str, str]]

class MSVCCompiler(CCompiler):
    """Concrete class that implements an interface to Microsoft Visual C++,
    as defined by the CCompiler abstract class.
    """

    compiler_type: ClassVar[str]
    executables: ClassVar[dict[Incomplete, Incomplete]]
    res_extension: ClassVar[str]
    initialized: bool
    def initialize(self, plat_name: str | None = None) -> None: ...
