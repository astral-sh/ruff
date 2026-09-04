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
the key "include-system-site-packages" set to "true" (case-insensitive),
the system-level prefixes will still also be searched for site-packages;
otherwise they won't.

Two kinds of configuration files are processed in each site-packages
directory:

- <name>.pth files extend sys.path with additional directories (one per
  line).  Lines starting with "import" are deprecated (see PEP 829).

- <name>.start files specify startup entry points using the pkg.mod:callable
  syntax.  These are resolved via pkgutil.resolve_name() and called with no
  arguments.

When called from main(), all .pth path extensions are applied before any
.start entry points are executed, ensuring that paths are available before
startup code runs.

See the documentation for the site module for full details:
https://docs.python.org/3/library/site.html
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
    """Set __file__ to an absolute path."""
def addpackage(sitedir: StrPath, name: StrPath, known_paths: set[str] | None) -> set[str] | None:  # undocumented
    """Process a .pth file within the site-packages directory."""

if sys.version_info >= (3, 15):
    class StartupState:
        """Per-batch accumulator for .pth and .start file processing.

A StartupState collects sys.path extensions, deprecated .pth import lines,
and .start entry points read from one or more site-packages directories.
Calling process() applies them in PEP 829 order: paths are added to
sys.path first, then import lines from .pth files (skipping any with a
matching .start), then entry points from .start files.

State lives entirely on the instance; there is no module-level pending
state.  This is what makes the module reentrancy-safe: a site.addsitedir()
call reached recursively from an exec'd import line or a .start entry
point operates on a different StartupState than the one being processed by
the outer call.

The internal data is intentionally private.  The lower-level write
methods (_record_sitedir(), _read_pth_file(), _read_start_file()) are
private to the site module; the public surface is addsitedir(),
addusersitepackages(), addsitepackages(), and process().
"""
        __slots__ = ("_known_paths", "_processed_sitedirs", "_path_entries", "_importexecs", "_entrypoints")
        def __init__(self, known_paths: set[str] | None = None) -> None:
            """Create an independent startup state.

*known_paths* is a set of case-normalized paths already present
on sys.path, used to avoid duplicate path entries.  When None
(the default), it is initialized from the current sys.path.

A caller-supplied set is stored by reference and mutated in place
as new paths are recorded; pass a fresh set per StartupState if
isolation across instances is required.
"""
        def addsitedir(self, sitedir: str) -> None:
            """Add a site directory and accumulate its .pth and .start startup data.

Read the .pth and .start files in *sitedir* and record their
sys.path extensions, deprecated .pth import lines, and .start entry
points on this state.  The recorded data is not applied until
process() is called.

Typically used to batch multiple site directories before a single
process() call, so that every sys.path extension is visible before
any startup code runs.  Reentrant calls reached from a .start entry
point or an exec'd .pth import line must not mutate the state
currently being processed; for those cases, use site.addsitedir()
instead, which always creates a fresh per-call state.
"""
        def addusersitepackages(self) -> None:
            """Add the per-user site-packages directory, if enabled.

The user site directory is added only when user site-packages are
enabled and the directory exists.  Its startup data is accumulated
for later processing by process().
"""
        def addsitepackages(self, prefixes: Iterable[str] | None = None) -> None:
            """Add global site-packages directories, if they exist.

Site-packages directories are computed from *prefixes*, or from the
global PREFIXES when *prefixes* is None.  Each directory's startup
data is accumulated for later processing by process().
"""
        def process(self) -> None:
            """Apply accumulated state in PEP 829 order.

Phase order matters: all .pth path extensions are applied to
sys.path *before* any import line or .start entry point runs, so
that an entry point may live in a module reachable only via a
.pth-extended path.
"""

def addsitedir(sitedir: str, known_paths: set[str] | None = None) -> None:
    """Add a site directory and process its startup files.

For batched processing across multiple site directories, build a
StartupState explicitly and call StartupState.addsitedir() on it; that
defers .pth/.start processing until a single StartupState.process() call.
"""
def addsitepackages(known_paths: set[str] | None, prefixes: Iterable[str] | None = None) -> set[str] | None:  # undocumented
    """Add global site-packages directories, if they exist.

Site-packages directories are computed from *prefixes*, or from the global
prefixes when *prefixes* is None.  Return *known_paths*, updated with any
paths added by addsitedir().
"""
def addusersitepackages(known_paths: set[str] | None) -> set[str] | None:  # undocumented
    """Add the per-user site-packages directory, if enabled.

The user site directory is added only when user site-packages are enabled
and the directory exists.  Return *known_paths*, updated with any paths
added by addsitedir().
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
def venv(known_paths: set[str] | None) -> set[str] | None:  # undocumented
    """Process pyvenv.cfg and add the venv site-packages, if applicable."""
