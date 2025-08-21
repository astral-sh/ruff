"""distutils.errors

Provides exceptions used by the Distutils modules.  Note that Distutils
modules may raise standard exceptions; in particular, SystemExit is
usually raised for errors that are obviously the end-user's fault
(eg. bad command-line arguments).

This module is safe to use in "from ... import *" mode; it only exports
symbols whose names start with "Distutils" and end with "Error".
"""

class DistutilsError(Exception):
    """The root of all Distutils evil."""

class DistutilsModuleError(DistutilsError):
    """Unable to load an expected module, or to find an expected class
    within some module (in particular, command modules and classes).
    """

class DistutilsClassError(DistutilsError):
    """Some command class (or possibly distribution class, if anyone
    feels a need to subclass Distribution) is found not to be holding
    up its end of the bargain, ie. implementing some part of the
    "command "interface.
    """

class DistutilsGetoptError(DistutilsError):
    """The option table provided to 'fancy_getopt()' is bogus."""

class DistutilsArgError(DistutilsError):
    """Raised by fancy_getopt in response to getopt.error -- ie. an
    error in the command line usage.
    """

class DistutilsFileError(DistutilsError):
    """Any problems in the filesystem: expected file not found, etc.
    Typically this is for problems that we detect before OSError
    could be raised.
    """

class DistutilsOptionError(DistutilsError):
    """Syntactic/semantic errors in command options, such as use of
    mutually conflicting options, or inconsistent options,
    badly-spelled values, etc.  No distinction is made between option
    values originating in the setup script, the command line, config
    files, or what-have-you -- but if we *know* something originated in
    the setup script, we'll raise DistutilsSetupError instead.
    """

class DistutilsSetupError(DistutilsError):
    """For errors that can be definitely blamed on the setup script,
    such as invalid keyword arguments to 'setup()'.
    """

class DistutilsPlatformError(DistutilsError):
    """We don't know how to do something on the current platform (but
    we do know how to do it on some platform) -- eg. trying to compile
    C files on a platform not supported by a CCompiler subclass.
    """

class DistutilsExecError(DistutilsError):
    """Any problems executing an external program (such as the C
    compiler, when compiling C files).
    """

class DistutilsInternalError(DistutilsError):
    """Internal inconsistencies or impossibilities (obviously, this
    should never be seen if the code is working!).
    """

class DistutilsTemplateError(DistutilsError):
    """Syntax error in a file list template."""

class DistutilsByteCompileError(DistutilsError):
    """Byte compile error."""

class CCompilerError(Exception):
    """Some compile/link operation failed."""

class PreprocessError(CCompilerError):
    """Failure to preprocess one or more C/C++ files."""

class CompileError(CCompilerError):
    """Failure to compile one or more C/C++ source files."""

class LibError(CCompilerError):
    """Failure to create a static library from one or more C/C++ object
    files.
    """

class LinkError(CCompilerError):
    """Failure to link one or more C/C++ object files into an executable
    or shared library file.
    """

class UnknownFileError(CCompilerError):
    """Attempt to process an unknown file type."""
