"""Append module search paths for third-party packages to sys.path.

****************************************************************
* This module is automatically imported during initialization. *
****************************************************************

This will append site-specific paths to the module search path.  On
Unix (including Mac OSX), it starts with sys.prefix and
sys.exec_prefix (if different) and appends
lib/python<version>/site-packages.
On other platforms (such as Windows), it tries each of the
prefixes directly, as well as with lib/site-packages appended.  The
resulting directories, if they exist, are appended to sys.path, and
also inspected for path configuration files.

If a file named "pyvenv.cfg" exists one directory above sys.executable,
sys.prefix and sys.exec_prefix are set to that directory and
it is also checked for site-packages (sys.base_prefix and
sys.base_exec_prefix will always be the "real" prefixes of the Python
installation). If "pyvenv.cfg" (a bootstrap configuration file) contains
the key "include-system-site-packages" set to anything other than "false"
(case-insensitive), the system-level prefixes will still also be
searched for site-packages; otherwise they won't.

All of the resulting site-specific directories, if they exist, are
appended to sys.path, and also inspected for path configuration
files.

A path configuration file is a file whose name has the form
<package>.pth; its contents are additional directories (one per line)
to be added to sys.path.  Non-existing directories (or
non-directories) are never added to sys.path; no directory is added to
sys.path more than once.  Blank lines and lines beginning with
'#' are skipped. Lines starting with 'import' are executed.

For example, suppose sys.prefix and sys.exec_prefix are set to
/usr/local and there is a directory /usr/local/lib/python2.5/site-packages
with three subdirectories, foo, bar and spam, and two path
configuration files, foo.pth and bar.pth.  Assume foo.pth contains the
following:

  # foo package configuration
  foo
  bar
  bletch

and bar.pth contains:

  # bar package configuration
  bar

Then the following directories are added to sys.path, in this order:

  /usr/local/lib/python2.5/site-packages/bar
  /usr/local/lib/python2.5/site-packages/foo

Note that bletch is omitted because it doesn't exist; bar precedes foo
because bar.pth comes alphabetically before foo.pth; and spam is
omitted because it is not mentioned in either path configuration file.

The readline module is also automatically configured to enable
completion for systems that support it.  This can be overridden in
sitecustomize, usercustomize or PYTHONSTARTUP.  Starting Python in
isolated mode (-I) disables automatic readline configuration.

After these operations, an attempt is made to import a module
named sitecustomize, which can perform arbitrary additional
site-specific customizations.  If this import fails with an
ImportError exception, it is silently ignored.
"""

import sys
from _typeshed import StrPath
from collections.abc import Iterable

PREFIXES: list[str]
ENABLE_USER_SITE: bool | None
USER_SITE: str | None
USER_BASE: str | None

def main() -> None:
    """Add standard site-specific directories to the module search path.

    This function is called automatically when this module is imported,
    unless the python interpreter was started with the -S flag.
    """

def abs_paths() -> None:  # undocumented
    """Set all module __file__ and __cached__ attributes to an absolute path"""

def addpackage(sitedir: StrPath, name: StrPath, known_paths: set[str] | None) -> set[str] | None:  # undocumented
    """Process a .pth file within the site-packages directory:
    For each line in the file, either combine it with sitedir to a path
    and add that to known_paths, or execute it if it starts with 'import '.
    """

def addsitedir(sitedir: str, known_paths: set[str] | None = None) -> None:
    """Add 'sitedir' argument to sys.path if missing and handle .pth files in
    'sitedir'
    """

def addsitepackages(known_paths: set[str] | None, prefixes: Iterable[str] | None = None) -> set[str] | None:  # undocumented
    """Add site-packages to sys.path"""

def addusersitepackages(known_paths: set[str] | None) -> set[str] | None:  # undocumented
    """Add a per user site-package to sys.path

    Each user has its own python directory with site-packages in the
    home directory.
    """

def check_enableusersite() -> bool | None:  # undocumented
    """Check if user site directory is safe for inclusion

    The function tests for the command line flag (including environment var),
    process uid/gid equal to effective uid/gid.

    None: Disabled for security reasons
    False: Disabled by user (command line option)
    True: Safe and enabled
    """

if sys.version_info >= (3, 13):
    def gethistoryfile() -> str:  # undocumented
        """Check if the PYTHON_HISTORY environment variable is set and define
        it as the .python_history file.  If PYTHON_HISTORY is not set, use the
        default .python_history file.
        """

def enablerlcompleter() -> None:  # undocumented
    """Enable default readline configuration on interactive prompts, by
    registering a sys.__interactivehook__.
    """

if sys.version_info >= (3, 13):
    def register_readline() -> None:  # undocumented
        """Configure readline completion on interactive prompts.

        If the readline module can be imported, the hook will set the Tab key
        as completion key and register ~/.python_history as history file.
        This can be overridden in the sitecustomize or usercustomize module,
        or in a PYTHONSTARTUP file.
        """

def execsitecustomize() -> None:  # undocumented
    """Run custom site specific code, if available."""

def execusercustomize() -> None:  # undocumented
    """Run custom user specific code, if available."""

def getsitepackages(prefixes: Iterable[str] | None = None) -> list[str]:
    """Returns a list containing all global site-packages directories.

    For each directory present in ``prefixes`` (or the global ``PREFIXES``),
    this function will find its `site-packages` subdirectory depending on the
    system environment, and will return a list of full paths.
    """

def getuserbase() -> str:
    """Returns the `user base` directory path.

    The `user base` directory can be used to store data. If the global
    variable ``USER_BASE`` is not initialized yet, this function will also set
    it.
    """

def getusersitepackages() -> str:
    """Returns the user-specific site-packages directory path.

    If the global variable ``USER_SITE`` is not initialized yet, this
    function will also set it.
    """

def makepath(*paths: StrPath) -> tuple[str, str]: ...  # undocumented
def removeduppaths() -> set[str]:  # undocumented
    """Remove duplicate entries from sys.path along with making them
    absolute
    """

def setcopyright() -> None:  # undocumented
    """Set 'copyright' and 'credits' in builtins"""

def sethelper() -> None: ...  # undocumented
def setquit() -> None:  # undocumented
    """Define new builtins 'quit' and 'exit'.

    These are objects which make the interpreter exit when called.
    The repr of each object contains a hint at how it works.

    """

def venv(known_paths: set[str] | None) -> set[str] | None: ...  # undocumented
