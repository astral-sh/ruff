"""distutils.bcppcompiler

Contains BorlandCCompiler, an implementation of the abstract CCompiler class
for the Borland C++ compiler.
"""

from distutils.ccompiler import CCompiler

class BCPPCompiler(CCompiler):
    """Concrete class that implements an interface to the Borland C/C++
    compiler, as defined by the CCompiler abstract class.
    """
