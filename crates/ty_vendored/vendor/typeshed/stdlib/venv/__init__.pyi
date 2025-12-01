"""
Virtual environment (venv) package for Python. Based on PEP 405.

Copyright (C) 2011-2014 Vinay Sajip.
Licensed to the PSF under a contributor agreement.
"""
import logging
import sys
from _typeshed import StrOrBytesPath
from collections.abc import Iterable, Sequence
from types import SimpleNamespace
from typing import Final

logger: logging.Logger

CORE_VENV_DEPS: Final[tuple[str, ...]]

class EnvBuilder:
    """
This class exists to allow virtual environment creation to be
customized. The constructor parameters determine the builder's
behaviour when called upon to create a virtual environment.

By default, the builder makes the system (global) site-packages dir
*un*available to the created environment.

If invoked using the Python -m option, the default is to use copying
on Windows platforms but symlinks elsewhere. If instantiated some
other way, the default is to *not* use symlinks.

:param system_site_packages: If True, the system (global) site-packages
                             dir is available to created environments.
:param clear: If True, delete the contents of the environment directory if
              it already exists, before environment creation.
:param symlinks: If True, attempt to symlink rather than copy files into
                 virtual environment.
:param upgrade: If True, upgrade an existing virtual environment.
:param with_pip: If True, ensure pip is installed in the virtual
                 environment
:param prompt: Alternative terminal prefix for the environment.
:param upgrade_deps: Update the base venv modules to the latest on PyPI
:param scm_ignore_files: Create ignore files for the SCMs specified by the
                         iterable.
"""
    system_site_packages: bool
    clear: bool
    symlinks: bool
    upgrade: bool
    with_pip: bool
    prompt: str | None

    if sys.version_info >= (3, 13):
        def __init__(
            self,
            system_site_packages: bool = False,
            clear: bool = False,
            symlinks: bool = False,
            upgrade: bool = False,
            with_pip: bool = False,
            prompt: str | None = None,
            upgrade_deps: bool = False,
            *,
            scm_ignore_files: Iterable[str] = ...,
        ) -> None: ...
    else:
        def __init__(
            self,
            system_site_packages: bool = False,
            clear: bool = False,
            symlinks: bool = False,
            upgrade: bool = False,
            with_pip: bool = False,
            prompt: str | None = None,
            upgrade_deps: bool = False,
        ) -> None: ...

    def create(self, env_dir: StrOrBytesPath) -> None:
        """
Create a virtual environment in a directory.

:param env_dir: The target directory to create an environment in.

"""
    def clear_directory(self, path: StrOrBytesPath) -> None: ...  # undocumented
    def ensure_directories(self, env_dir: StrOrBytesPath) -> SimpleNamespace:
        """
Create the directories for the environment.

Returns a context object which holds paths in the environment,
for use by subsequent logic.
"""
    def create_configuration(self, context: SimpleNamespace) -> None:
        """
Create a configuration file indicating where the environment's Python
was copied from, and whether the system site-packages should be made
available in the environment.

:param context: The information for the environment creation request
                being processed.
"""
    def symlink_or_copy(
        self, src: StrOrBytesPath, dst: StrOrBytesPath, relative_symlinks_ok: bool = False
    ) -> None:  # undocumented
        """
Try symlinking a file, and if that fails, fall back to copying.
(Unused on Windows, because we can't just copy a failed symlink file: we
switch to a different set of files instead.)
"""
    def setup_python(self, context: SimpleNamespace) -> None:
        """
Set up a Python executable in the environment.

:param context: The information for the environment creation request
                being processed.
"""
    def _setup_pip(self, context: SimpleNamespace) -> None:  # undocumented
        """Installs or upgrades pip in a virtual environment
"""
    def setup_scripts(self, context: SimpleNamespace) -> None:
        """
Set up scripts into the created environment from a directory.

This method installs the default scripts into the environment
being created. You can prevent the default installation by overriding
this method if you really need to, or if you need to specify
a different location for the scripts to install. By default, the
'scripts' directory in the venv package is used as the source of
scripts to install.
"""
    def post_setup(self, context: SimpleNamespace) -> None:
        """
Hook for post-setup modification of the venv. Subclasses may install
additional packages or scripts here, add activation shell scripts, etc.

:param context: The information for the environment creation request
                being processed.
"""
    def replace_variables(self, text: str, context: SimpleNamespace) -> str:  # undocumented
        """
Replace variable placeholders in script text with context-specific
variables.

Return the text passed in , but with variables replaced.

:param text: The text in which to replace placeholder variables.
:param context: The information for the environment creation request
                being processed.
"""
    def install_scripts(self, context: SimpleNamespace, path: str) -> None:
        """
Install scripts into the created environment from a directory.

:param context: The information for the environment creation request
                being processed.
:param path:    Absolute pathname of a directory containing script.
                Scripts in the 'common' subdirectory of this directory,
                and those in the directory named for the platform
                being run on, are installed in the created environment.
                Placeholder variables are replaced with environment-
                specific values.
"""
    def upgrade_dependencies(self, context: SimpleNamespace) -> None: ...
    if sys.version_info >= (3, 13):
        def create_git_ignore_file(self, context: SimpleNamespace) -> None:
            """
Create a .gitignore file in the environment directory.

The contents of the file cause the entire environment directory to be
ignored by git.
"""

if sys.version_info >= (3, 13):
    def create(
        env_dir: StrOrBytesPath,
        system_site_packages: bool = False,
        clear: bool = False,
        symlinks: bool = False,
        with_pip: bool = False,
        prompt: str | None = None,
        upgrade_deps: bool = False,
        *,
        scm_ignore_files: Iterable[str] = ...,
    ) -> None:
        """Create a virtual environment in a directory.
"""

else:
    def create(
        env_dir: StrOrBytesPath,
        system_site_packages: bool = False,
        clear: bool = False,
        symlinks: bool = False,
        with_pip: bool = False,
        prompt: str | None = None,
        upgrade_deps: bool = False,
    ) -> None:
        """Create a virtual environment in a directory.
"""

def main(args: Sequence[str] | None = None) -> None: ...
