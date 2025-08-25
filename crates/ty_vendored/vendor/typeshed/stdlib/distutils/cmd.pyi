"""distutils.cmd

Provides the Command class, the base class for the command classes
in the distutils.command package.
"""

from _typeshed import BytesPath, StrOrBytesPath, StrPath, Unused
from abc import abstractmethod
from collections.abc import Callable, Iterable
from distutils.command.bdist import bdist
from distutils.command.bdist_dumb import bdist_dumb
from distutils.command.bdist_rpm import bdist_rpm
from distutils.command.build import build
from distutils.command.build_clib import build_clib
from distutils.command.build_ext import build_ext
from distutils.command.build_py import build_py
from distutils.command.build_scripts import build_scripts
from distutils.command.check import check
from distutils.command.clean import clean
from distutils.command.config import config
from distutils.command.install import install
from distutils.command.install_data import install_data
from distutils.command.install_egg_info import install_egg_info
from distutils.command.install_headers import install_headers
from distutils.command.install_lib import install_lib
from distutils.command.install_scripts import install_scripts
from distutils.command.register import register
from distutils.command.sdist import sdist
from distutils.command.upload import upload
from distutils.dist import Distribution
from distutils.file_util import _BytesPathT, _StrPathT
from typing import Any, ClassVar, Literal, TypeVar, overload
from typing_extensions import TypeVarTuple, Unpack

_CommandT = TypeVar("_CommandT", bound=Command)
_Ts = TypeVarTuple("_Ts")

class Command:
    """Abstract base class for defining command classes, the "worker bees"
    of the Distutils.  A useful analogy for command classes is to think of
    them as subroutines with local variables called "options".  The options
    are "declared" in 'initialize_options()' and "defined" (given their
    final values, aka "finalized") in 'finalize_options()', both of which
    must be defined by every command class.  The distinction between the
    two is necessary because option values might come from the outside
    world (command line, config file, ...), and any options dependent on
    other options must be computed *after* these outside influences have
    been processed -- hence 'finalize_options()'.  The "body" of the
    subroutine, where it does all its work based on the values of its
    options, is the 'run()' method, which must also be implemented by every
    command class.
    """

    dry_run: bool | Literal[0, 1]  # Exposed from __getattr_. Same as Distribution.dry_run
    distribution: Distribution
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
    def __init__(self, dist: Distribution) -> None:
        """Create and initialize a new Command object.  Most importantly,
        invokes the 'initialize_options()' method, which is the real
        initializer and depends on the actual command being
        instantiated.
        """

    @abstractmethod
    def initialize_options(self) -> None:
        """Set default values for all the options that this command
        supports.  Note that these defaults may be overridden by other
        commands, by the setup script, by config files, or by the
        command-line.  Thus, this is not the place to code dependencies
        between options; generally, 'initialize_options()' implementations
        are just a bunch of "self.foo = None" assignments.

        This method must be implemented by all command classes.
        """

    @abstractmethod
    def finalize_options(self) -> None:
        """Set final values for all the options that this command supports.
        This is always called as late as possible, ie.  after any option
        assignments from the command-line or from other commands have been
        done.  Thus, this is the place to code option dependencies: if
        'foo' depends on 'bar', then it is safe to set 'foo' from 'bar' as
        long as 'foo' still has the same value it was assigned in
        'initialize_options()'.

        This method must be implemented by all command classes.
        """

    @abstractmethod
    def run(self) -> None:
        """A command's raison d'etre: carry out the action it exists to
        perform, controlled by the options initialized in
        'initialize_options()', customized by other commands, the setup
        script, the command-line, and config files, and finalized in
        'finalize_options()'.  All terminal output and filesystem
        interaction should be done by 'run()'.

        This method must be implemented by all command classes.
        """

    def announce(self, msg: str, level: int = 1) -> None:
        """If the current verbosity level is of greater than or equal to
        'level' print 'msg' to stdout.
        """

    def debug_print(self, msg: str) -> None:
        """Print 'msg' to stdout if the global DEBUG (taken from the
        DISTUTILS_DEBUG environment variable) flag is true.
        """

    def ensure_string(self, option: str, default: str | None = None) -> None:
        """Ensure that 'option' is a string; if not defined, set it to
        'default'.
        """

    def ensure_string_list(self, option: str) -> None:
        """Ensure that 'option' is a list of strings.  If 'option' is
        currently a string, we split it either on /,\\s*/ or /\\s+/, so
        "foo bar baz", "foo,bar,baz", and "foo,   bar baz" all become
        ["foo", "bar", "baz"].
        """

    def ensure_filename(self, option: str) -> None:
        """Ensure that 'option' is the name of an existing file."""

    def ensure_dirname(self, option: str) -> None: ...
    def get_command_name(self) -> str: ...
    def set_undefined_options(self, src_cmd: str, *option_pairs: tuple[str, str]) -> None:
        """Set the values of any "undefined" options from corresponding
        option values in some other command object.  "Undefined" here means
        "is None", which is the convention used to indicate that an option
        has not been changed between 'initialize_options()' and
        'finalize_options()'.  Usually called from 'finalize_options()' for
        options that depend on some other command rather than another
        option of the same command.  'src_cmd' is the other command from
        which option values will be taken (a command object will be created
        for it if necessary); the remaining arguments are
        '(src_option,dst_option)' tuples which mean "take the value of
        'src_option' in the 'src_cmd' command object, and copy it to
        'dst_option' in the current command object".
        """
    # NOTE: This list comes directly from the distutils/command folder. Minus bdist_msi and bdist_wininst.
    @overload
    def get_finalized_command(self, command: Literal["bdist"], create: bool | Literal[0, 1] = 1) -> bdist:
        """Wrapper around Distribution's 'get_command_obj()' method: find
        (create if necessary and 'create' is true) the command object for
        'command', call its 'ensure_finalized()' method, and return the
        finalized command object.
        """

    @overload
    def get_finalized_command(self, command: Literal["bdist_dumb"], create: bool | Literal[0, 1] = 1) -> bdist_dumb: ...
    @overload
    def get_finalized_command(self, command: Literal["bdist_rpm"], create: bool | Literal[0, 1] = 1) -> bdist_rpm: ...
    @overload
    def get_finalized_command(self, command: Literal["build"], create: bool | Literal[0, 1] = 1) -> build: ...
    @overload
    def get_finalized_command(self, command: Literal["build_clib"], create: bool | Literal[0, 1] = 1) -> build_clib: ...
    @overload
    def get_finalized_command(self, command: Literal["build_ext"], create: bool | Literal[0, 1] = 1) -> build_ext: ...
    @overload
    def get_finalized_command(self, command: Literal["build_py"], create: bool | Literal[0, 1] = 1) -> build_py: ...
    @overload
    def get_finalized_command(self, command: Literal["build_scripts"], create: bool | Literal[0, 1] = 1) -> build_scripts: ...
    @overload
    def get_finalized_command(self, command: Literal["check"], create: bool | Literal[0, 1] = 1) -> check: ...
    @overload
    def get_finalized_command(self, command: Literal["clean"], create: bool | Literal[0, 1] = 1) -> clean: ...
    @overload
    def get_finalized_command(self, command: Literal["config"], create: bool | Literal[0, 1] = 1) -> config: ...
    @overload
    def get_finalized_command(self, command: Literal["install"], create: bool | Literal[0, 1] = 1) -> install: ...
    @overload
    def get_finalized_command(self, command: Literal["install_data"], create: bool | Literal[0, 1] = 1) -> install_data: ...
    @overload
    def get_finalized_command(
        self, command: Literal["install_egg_info"], create: bool | Literal[0, 1] = 1
    ) -> install_egg_info: ...
    @overload
    def get_finalized_command(self, command: Literal["install_headers"], create: bool | Literal[0, 1] = 1) -> install_headers: ...
    @overload
    def get_finalized_command(self, command: Literal["install_lib"], create: bool | Literal[0, 1] = 1) -> install_lib: ...
    @overload
    def get_finalized_command(self, command: Literal["install_scripts"], create: bool | Literal[0, 1] = 1) -> install_scripts: ...
    @overload
    def get_finalized_command(self, command: Literal["register"], create: bool | Literal[0, 1] = 1) -> register: ...
    @overload
    def get_finalized_command(self, command: Literal["sdist"], create: bool | Literal[0, 1] = 1) -> sdist: ...
    @overload
    def get_finalized_command(self, command: Literal["upload"], create: bool | Literal[0, 1] = 1) -> upload: ...
    @overload
    def get_finalized_command(self, command: str, create: bool | Literal[0, 1] = 1) -> Command: ...
    @overload
    def reinitialize_command(self, command: Literal["bdist"], reinit_subcommands: bool | Literal[0, 1] = 0) -> bdist: ...
    @overload
    def reinitialize_command(
        self, command: Literal["bdist_dumb"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> bdist_dumb: ...
    @overload
    def reinitialize_command(self, command: Literal["bdist_rpm"], reinit_subcommands: bool | Literal[0, 1] = 0) -> bdist_rpm: ...
    @overload
    def reinitialize_command(self, command: Literal["build"], reinit_subcommands: bool | Literal[0, 1] = 0) -> build: ...
    @overload
    def reinitialize_command(
        self, command: Literal["build_clib"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> build_clib: ...
    @overload
    def reinitialize_command(self, command: Literal["build_ext"], reinit_subcommands: bool | Literal[0, 1] = 0) -> build_ext: ...
    @overload
    def reinitialize_command(self, command: Literal["build_py"], reinit_subcommands: bool | Literal[0, 1] = 0) -> build_py: ...
    @overload
    def reinitialize_command(
        self, command: Literal["build_scripts"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> build_scripts: ...
    @overload
    def reinitialize_command(self, command: Literal["check"], reinit_subcommands: bool | Literal[0, 1] = 0) -> check: ...
    @overload
    def reinitialize_command(self, command: Literal["clean"], reinit_subcommands: bool | Literal[0, 1] = 0) -> clean: ...
    @overload
    def reinitialize_command(self, command: Literal["config"], reinit_subcommands: bool | Literal[0, 1] = 0) -> config: ...
    @overload
    def reinitialize_command(self, command: Literal["install"], reinit_subcommands: bool | Literal[0, 1] = 0) -> install: ...
    @overload
    def reinitialize_command(
        self, command: Literal["install_data"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> install_data: ...
    @overload
    def reinitialize_command(
        self, command: Literal["install_egg_info"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> install_egg_info: ...
    @overload
    def reinitialize_command(
        self, command: Literal["install_headers"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> install_headers: ...
    @overload
    def reinitialize_command(
        self, command: Literal["install_lib"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> install_lib: ...
    @overload
    def reinitialize_command(
        self, command: Literal["install_scripts"], reinit_subcommands: bool | Literal[0, 1] = 0
    ) -> install_scripts: ...
    @overload
    def reinitialize_command(self, command: Literal["register"], reinit_subcommands: bool | Literal[0, 1] = 0) -> register: ...
    @overload
    def reinitialize_command(self, command: Literal["sdist"], reinit_subcommands: bool | Literal[0, 1] = 0) -> sdist: ...
    @overload
    def reinitialize_command(self, command: Literal["upload"], reinit_subcommands: bool | Literal[0, 1] = 0) -> upload: ...
    @overload
    def reinitialize_command(self, command: str, reinit_subcommands: bool | Literal[0, 1] = 0) -> Command: ...
    @overload
    def reinitialize_command(self, command: _CommandT, reinit_subcommands: bool | Literal[0, 1] = 0) -> _CommandT: ...
    def run_command(self, command: str) -> None:
        """Run some other command: uses the 'run_command()' method of
        Distribution, which creates and finalizes the command object if
        necessary and then invokes its 'run()' method.
        """

    def get_sub_commands(self) -> list[str]:
        """Determine the sub-commands that are relevant in the current
        distribution (ie., that need to be run).  This is based on the
        'sub_commands' class attribute: each tuple in that list may include
        a method that we call to determine if the subcommand needs to be
        run for the current distribution.  Return a list of command names.
        """

    def warn(self, msg: str) -> None: ...
    def execute(
        self, func: Callable[[Unpack[_Ts]], Unused], args: tuple[Unpack[_Ts]], msg: str | None = None, level: int = 1
    ) -> None: ...
    def mkpath(self, name: str, mode: int = 0o777) -> None: ...
    @overload
    def copy_file(
        self,
        infile: StrPath,
        outfile: _StrPathT,
        preserve_mode: bool | Literal[0, 1] = 1,
        preserve_times: bool | Literal[0, 1] = 1,
        link: str | None = None,
        level: Unused = 1,
    ) -> tuple[_StrPathT | str, bool]:
        """Copy a file respecting verbose, dry-run and force flags.  (The
        former two default to whatever is in the Distribution object, and
        the latter defaults to false for commands that don't define it.)
        """

    @overload
    def copy_file(
        self,
        infile: BytesPath,
        outfile: _BytesPathT,
        preserve_mode: bool | Literal[0, 1] = 1,
        preserve_times: bool | Literal[0, 1] = 1,
        link: str | None = None,
        level: Unused = 1,
    ) -> tuple[_BytesPathT | bytes, bool]: ...
    def copy_tree(
        self,
        infile: StrPath,
        outfile: str,
        preserve_mode: bool | Literal[0, 1] = 1,
        preserve_times: bool | Literal[0, 1] = 1,
        preserve_symlinks: bool | Literal[0, 1] = 0,
        level: Unused = 1,
    ) -> list[str]:
        """Copy an entire directory tree respecting verbose, dry-run,
        and force flags.
        """

    @overload
    def move_file(self, src: StrPath, dst: _StrPathT, level: Unused = 1) -> _StrPathT | str:
        """Move a file respecting dry-run flag."""

    @overload
    def move_file(self, src: BytesPath, dst: _BytesPathT, level: Unused = 1) -> _BytesPathT | bytes: ...
    def spawn(self, cmd: Iterable[str], search_path: bool | Literal[0, 1] = 1, level: Unused = 1) -> None:
        """Spawn an external command respecting dry-run flag."""

    @overload
    def make_archive(
        self,
        base_name: str,
        format: str,
        root_dir: StrOrBytesPath | None = None,
        base_dir: str | None = None,
        owner: str | None = None,
        group: str | None = None,
    ) -> str: ...
    @overload
    def make_archive(
        self,
        base_name: StrPath,
        format: str,
        root_dir: StrOrBytesPath,
        base_dir: str | None = None,
        owner: str | None = None,
        group: str | None = None,
    ) -> str: ...
    def make_file(
        self,
        infiles: str | list[str] | tuple[str, ...],
        outfile: StrOrBytesPath,
        func: Callable[[Unpack[_Ts]], Unused],
        args: tuple[Unpack[_Ts]],
        exec_msg: str | None = None,
        skip_msg: str | None = None,
        level: Unused = 1,
    ) -> None:
        """Special case of 'execute()' for operations that process one or
        more input files and generate one output file.  Works just like
        'execute()', except the operation is skipped and a different
        message printed if 'outfile' already exists and is newer than all
        files listed in 'infiles'.  If the command defined 'self.force',
        and it is true, then the command is unconditionally run -- does no
        timestamp checks.
        """

    def ensure_finalized(self) -> None: ...
    def dump_options(self, header=None, indent: str = "") -> None: ...
