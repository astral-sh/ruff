"""distutils.command.sdist

Implements the Distutils 'sdist' command (create a source distribution).
"""

from _typeshed import Incomplete, Unused
from collections.abc import Callable
from typing import Any, ClassVar

from ..cmd import Command

def show_formats() -> None:
    """Print all possible values for the 'formats' option (used by
    the "--help-formats" command-line option).
    """

class sdist(Command):
    description: str
    def checking_metadata(self):
        """Callable used for the check sub-command.

        Placed here so user_options can view it
        """
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    negative_opt: ClassVar[dict[str, str]]
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
    READMES: ClassVar[tuple[str, ...]]
    template: Incomplete
    manifest: Incomplete
    use_defaults: int
    prune: int
    manifest_only: int
    force_manifest: int
    formats: Incomplete
    keep_temp: int
    dist_dir: Incomplete
    archive_files: Incomplete
    metadata_check: int
    owner: Incomplete
    group: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    filelist: Incomplete
    def run(self) -> None: ...
    def check_metadata(self) -> None:
        """Deprecated API."""

    def get_file_list(self) -> None:
        """Figure out the list of files to include in the source
        distribution, and put it in 'self.filelist'.  This might involve
        reading the manifest template (and writing the manifest), or just
        reading the manifest, or just using the default file set -- it all
        depends on the user's options.
        """

    def add_defaults(self) -> None:
        """Add all the default files to self.filelist:
          - README or README.txt
          - setup.py
          - test/test*.py
          - all pure Python modules mentioned in setup script
          - all files pointed by package_data (build_py)
          - all files defined in data_files.
          - all files defined as scripts.
          - all C sources listed as part of extensions or C libraries
            in the setup script (doesn't catch C headers!)
        Warns if (README or README.txt) or setup.py are missing; everything
        else is optional.
        """

    def read_template(self) -> None:
        """Read and parse manifest template file named by self.template.

        (usually "MANIFEST.in") The parsing and processing is done by
        'self.filelist', which updates itself accordingly.
        """

    def prune_file_list(self) -> None:
        """Prune off branches that might slip into the file list as created
        by 'read_template()', but really don't belong there:
          * the build tree (typically "build")
          * the release tree itself (only an issue if we ran "sdist"
            previously with --keep-temp, or it aborted)
          * any RCS, CVS, .svn, .hg, .git, .bzr, _darcs directories
        """

    def write_manifest(self) -> None:
        """Write the file list in 'self.filelist' (presumably as filled in
        by 'add_defaults()' and 'read_template()') to the manifest file
        named by 'self.manifest'.
        """

    def read_manifest(self) -> None:
        """Read the manifest file (named by 'self.manifest') and use it to
        fill in 'self.filelist', the list of files to include in the source
        distribution.
        """

    def make_release_tree(self, base_dir, files) -> None:
        """Create the directory tree that will become the source
        distribution archive.  All directories implied by the filenames in
        'files' are created under 'base_dir', and then we hard link or copy
        (if hard linking is unavailable) those files into place.
        Essentially, this duplicates the developer's source tree, but in a
        directory named after the distribution, containing only the files
        to be distributed.
        """

    def make_distribution(self) -> None:
        """Create the source distribution(s).  First, we create the release
        tree with 'make_release_tree()'; then, we create all required
        archive files (according to 'self.formats') from the release tree.
        Finally, we clean up by blowing away the release tree (unless
        'self.keep_temp' is true).  The list of archive files created is
        stored so it can be retrieved later by 'get_archive_files()'.
        """

    def get_archive_files(self):
        """Return the list of archive files created when the command
        was run, or None if the command hasn't run yet.
        """
