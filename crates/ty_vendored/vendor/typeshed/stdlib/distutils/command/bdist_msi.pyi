"""
Implements the bdist_msi command.
"""

import sys
from _typeshed import Incomplete
from typing import ClassVar, Literal

from ..cmd import Command

if sys.platform == "win32":
    from msilib import Control, Dialog

    class PyDialog(Dialog):
        """Dialog class with a fixed layout: controls at the top, then a ruler,
        then a list of buttons: back, next, cancel. Optionally a bitmap at the
        left.
        """

        def __init__(self, *args, **kw) -> None:
            """Dialog(database, name, x, y, w, h, attributes, title, first,
            default, cancel, bitmap=true)
            """

        def title(self, title) -> None:
            """Set the title text of the dialog at the top."""

        def back(self, title, next, name: str = "Back", active: bool | Literal[0, 1] = 1) -> Control:
            """Add a back button with a given title, the tab-next button,
            its name in the Control table, possibly initially disabled.

            Return the button, so that events can be associated
            """

        def cancel(self, title, next, name: str = "Cancel", active: bool | Literal[0, 1] = 1) -> Control:
            """Add a cancel button with a given title, the tab-next button,
            its name in the Control table, possibly initially disabled.

            Return the button, so that events can be associated
            """

        def next(self, title, next, name: str = "Next", active: bool | Literal[0, 1] = 1) -> Control:
            """Add a Next button with a given title, the tab-next button,
            its name in the Control table, possibly initially disabled.

            Return the button, so that events can be associated
            """

        def xbutton(self, name, title, next, xpos) -> Control:
            """Add a button with a given title, the tab-next button,
            its name in the Control table, giving its x position; the
            y-position is aligned with the other buttons.

            Return the button, so that events can be associated
            """

    class bdist_msi(Command):
        description: str
        user_options: ClassVar[list[tuple[str, str | None, str]]]
        boolean_options: ClassVar[list[str]]
        all_versions: Incomplete
        other_version: str
        def __init__(self, *args, **kw) -> None: ...
        bdist_dir: Incomplete
        plat_name: Incomplete
        keep_temp: int
        no_target_compile: int
        no_target_optimize: int
        target_version: Incomplete
        dist_dir: Incomplete
        skip_build: Incomplete
        install_script: Incomplete
        pre_install_script: Incomplete
        versions: Incomplete
        def initialize_options(self) -> None: ...
        install_script_key: Incomplete
        def finalize_options(self) -> None: ...
        db: Incomplete
        def run(self) -> None: ...
        def add_files(self) -> None: ...
        def add_find_python(self) -> None:
            """Adds code to the installer to compute the location of Python.

            Properties PYTHON.MACHINE.X.Y and PYTHON.USER.X.Y will be set from the
            registry for each version of Python.

            Properties TARGETDIRX.Y will be set from PYTHON.USER.X.Y if defined,
            else from PYTHON.MACHINE.X.Y.

            Properties PYTHONX.Y will be set to TARGETDIRX.Y\\python.exe
            """

        def add_scripts(self) -> None: ...
        def add_ui(self) -> None: ...
        def get_installer_filename(self, fullname): ...
