"""distutils.msvccompiler

Contains MSVCCompiler, an implementation of the abstract CCompiler class
for the Microsoft Visual Studio.
"""

from distutils.ccompiler import CCompiler

class MSVCCompiler(CCompiler):
    """Concrete class that implements an interface to Microsoft Visual C++,
    as defined by the CCompiler abstract class.
    """
