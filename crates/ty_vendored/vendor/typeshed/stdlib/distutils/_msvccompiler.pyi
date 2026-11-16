from _typeshed import Incomplete
from distutils.ccompiler import CCompiler
from typing import ClassVar, Final

PLAT_SPEC_TO_RUNTIME: Final[dict[str, str]]
PLAT_TO_VCVARS: Final[dict[str, str]]

class MSVCCompiler(CCompiler):
    compiler_type: ClassVar[str]
    executables: ClassVar[dict[Incomplete, Incomplete]]
    res_extension: ClassVar[str]
    initialized: bool
    def initialize(self, plat_name: str | None = None) -> None: ...
