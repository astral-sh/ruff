"""distutils.util

Miscellaneous utility functions -- anything that doesn't fit into
one of the other *util.py modules.
"""

from _typeshed import StrPath, Unused
from collections.abc import Callable, Container, Iterable, Mapping
from typing import Any, Literal
from typing_extensions import TypeVarTuple, Unpack

_Ts = TypeVarTuple("_Ts")

def get_host_platform() -> str:
    """Return a string that identifies the current platform.  This is used mainly to
    distinguish platform-specific build directories and platform-specific built
    distributions.  Typically includes the OS name and version and the
    architecture (as supplied by 'os.uname()'), although the exact information
    included depends on the OS; eg. on Linux, the kernel version isn't
    particularly important.

    Examples of returned values:
       linux-i586
       linux-alpha (?)
       solaris-2.6-sun4u

    Windows will return one of:
       win-amd64 (64bit Windows on AMD64 (aka x86_64, Intel64, EM64T, etc)
       win32 (all others - specifically, sys.platform is returned)

    For other non-POSIX platforms, currently just returns 'sys.platform'.

    """

def get_platform() -> str: ...
def convert_path(pathname: str) -> str:
    """Return 'pathname' as a name that will work on the native filesystem,
    i.e. split it on '/' and put it back together again using the current
    directory separator.  Needed because filenames in the setup script are
    always supplied in Unix style, and have to be converted to the local
    convention before we can actually use them in the filesystem.  Raises
    ValueError on non-Unix-ish systems if 'pathname' either starts or
    ends with a slash.
    """

def change_root(new_root: StrPath, pathname: StrPath) -> str:
    """Return 'pathname' with 'new_root' prepended.  If 'pathname' is
    relative, this is equivalent to "os.path.join(new_root,pathname)".
    Otherwise, it requires making 'pathname' relative and then joining the
    two, which is tricky on DOS/Windows and Mac OS.
    """

def check_environ() -> None:
    """Ensure that 'os.environ' has all the environment variables we
    guarantee that users can use in config files, command-line options,
    etc.  Currently this includes:
      HOME - user's home directory (Unix only)
      PLAT - description of the current platform, including hardware
             and OS (see 'get_platform()')
    """

def subst_vars(s: str, local_vars: Mapping[str, str]) -> None:
    """Perform shell/Perl-style variable substitution on 'string'.  Every
    occurrence of '$' followed by a name is considered a variable, and
    variable is substituted by the value found in the 'local_vars'
    dictionary, or in 'os.environ' if it's not in 'local_vars'.
    'os.environ' is first checked/augmented to guarantee that it contains
    certain values: see 'check_environ()'.  Raise ValueError for any
    variables not found in either 'local_vars' or 'os.environ'.
    """

def split_quoted(s: str) -> list[str]:
    """Split a string up according to Unix shell-like rules for quotes and
    backslashes.  In short: words are delimited by spaces, as long as those
    spaces are not escaped by a backslash, or inside a quoted string.
    Single and double quotes are equivalent, and the quote characters can
    be backslash-escaped.  The backslash is stripped from any two-character
    escape sequence, leaving only the escaped character.  The quote
    characters are stripped from any quoted string.  Returns a list of
    words.
    """

def execute(
    func: Callable[[Unpack[_Ts]], Unused],
    args: tuple[Unpack[_Ts]],
    msg: str | None = None,
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
) -> None:
    """Perform some action that affects the outside world (eg.  by
    writing to the filesystem).  Such actions are special because they
    are disabled by the 'dry_run' flag.  This method takes care of all
    that bureaucracy for you; all you have to do is supply the
    function to call and an argument tuple for it (to embody the
    "external action" being performed), and an optional message to
    print.
    """

def strtobool(val: str) -> Literal[0, 1]:
    """Convert a string representation of truth to true (1) or false (0).

    True values are 'y', 'yes', 't', 'true', 'on', and '1'; false values
    are 'n', 'no', 'f', 'false', 'off', and '0'.  Raises ValueError if
    'val' is anything else.
    """

def byte_compile(
    py_files: list[str],
    optimize: int = 0,
    force: bool | Literal[0, 1] = 0,
    prefix: str | None = None,
    base_dir: str | None = None,
    verbose: bool | Literal[0, 1] = 1,
    dry_run: bool | Literal[0, 1] = 0,
    direct: bool | None = None,
) -> None:
    """Byte-compile a collection of Python source files to .pyc
    files in a __pycache__ subdirectory.  'py_files' is a list
    of files to compile; any files that don't end in ".py" are silently
    skipped.  'optimize' must be one of the following:
      0 - don't optimize
      1 - normal optimization (like "python -O")
      2 - extra optimization (like "python -OO")
    If 'force' is true, all files are recompiled regardless of
    timestamps.

    The source filename encoded in each bytecode file defaults to the
    filenames listed in 'py_files'; you can modify these with 'prefix' and
    'basedir'.  'prefix' is a string that will be stripped off of each
    source filename, and 'base_dir' is a directory name that will be
    prepended (after 'prefix' is stripped).  You can supply either or both
    (or neither) of 'prefix' and 'base_dir', as you wish.

    If 'dry_run' is true, doesn't actually do anything that would
    affect the filesystem.

    Byte-compilation is either done directly in this interpreter process
    with the standard py_compile module, or indirectly by writing a
    temporary script and executing it.  Normally, you should let
    'byte_compile()' figure out to use direct compilation or not (see
    the source for details).  The 'direct' flag is used by the script
    generated in indirect mode; unless you know what you're doing, leave
    it set to None.
    """

def rfc822_escape(header: str) -> str:
    """Return a version of the string escaped for inclusion in an
    RFC-822 header, by ensuring there are 8 spaces space after each newline.
    """

def run_2to3(
    files: Iterable[str],
    fixer_names: Iterable[str] | None = None,
    options: Mapping[str, Any] | None = None,
    explicit: Unused = None,
) -> None:
    """Invoke 2to3 on a list of Python files.
    The files should all come from the build area, as the
    modification is done in-place. To reduce the build time,
    only files modified since the last invocation of this
    function should be passed in the files argument.
    """

def copydir_run_2to3(
    src: StrPath,
    dest: StrPath,
    template: str | None = None,
    fixer_names: Iterable[str] | None = None,
    options: Mapping[str, Any] | None = None,
    explicit: Container[str] | None = None,
) -> list[str]:
    """Recursively copy a directory, only copying new and changed files,
    running run_2to3 over all newly copied Python modules afterward.

    If you give a template string, it's parsed like a MANIFEST.in.
    """

class Mixin2to3:
    """Mixin class for commands that run 2to3.
    To configure 2to3, setup scripts may either change
    the class variables, or inherit from individual commands
    to override how 2to3 is invoked.
    """

    fixer_names: Iterable[str] | None
    options: Mapping[str, Any] | None
    explicit: Container[str] | None
    def run_2to3(self, files: Iterable[str]) -> None: ...
