"""Interfaces for launching and remotely controlling web browsers."""

import sys
from abc import abstractmethod
from collections.abc import Callable, Sequence
from typing import Literal
from typing_extensions import deprecated

__all__ = ["Error", "open", "open_new", "open_new_tab", "get", "register"]

class Error(Exception): ...

def register(
    name: str, klass: Callable[[], BaseBrowser] | None, instance: BaseBrowser | None = None, *, preferred: bool = False
) -> None:
    """Register a browser connector."""

def get(using: str | None = None) -> BaseBrowser:
    """Return a browser launcher instance appropriate for the environment."""

def open(url: str, new: int = 0, autoraise: bool = True) -> bool:
    """Display url using the default browser.

    If possible, open url in a location determined by new.
    - 0: the same browser window (the default).
    - 1: a new browser window.
    - 2: a new browser page ("tab").
    If possible, autoraise raises the window (the default) or not.

    If opening the browser succeeds, return True.
    If there is a problem, return False.
    """

def open_new(url: str) -> bool:
    """Open url in a new window of the default browser.

    If not possible, then open url in the only browser window.
    """

def open_new_tab(url: str) -> bool:
    """Open url in a new page ("tab") of the default browser.

    If not possible, then the behavior becomes equivalent to open_new().
    """

class BaseBrowser:
    """Parent class for all browsers. Do not use directly."""

    args: list[str]
    name: str
    basename: str
    def __init__(self, name: str = "") -> None: ...
    @abstractmethod
    def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...
    def open_new(self, url: str) -> bool: ...
    def open_new_tab(self, url: str) -> bool: ...

class GenericBrowser(BaseBrowser):
    """Class for all browsers started with a command
    and without remote functionality.
    """

    def __init__(self, name: str | Sequence[str]) -> None: ...
    def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

class BackgroundBrowser(GenericBrowser):
    """Class for all browsers which are to be started in the
    background.
    """

class UnixBrowser(BaseBrowser):
    """Parent class for all Unix browsers with remote functionality."""

    def open(self, url: str, new: Literal[0, 1, 2] = 0, autoraise: bool = True) -> bool: ...  # type: ignore[override]
    raise_opts: list[str] | None
    background: bool
    redirect_stdout: bool
    remote_args: list[str]
    remote_action: str
    remote_action_newwin: str
    remote_action_newtab: str

class Mozilla(UnixBrowser):
    """Launcher class for Mozilla browsers."""

if sys.version_info < (3, 12):
    class Galeon(UnixBrowser):
        """Launcher class for Galeon/Epiphany browsers."""

        raise_opts: list[str]

    class Grail(BaseBrowser):
        def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

class Chrome(UnixBrowser):
    """Launcher class for Google Chrome browser."""

class Opera(UnixBrowser):
    """Launcher class for Opera browser."""

class Elinks(UnixBrowser):
    """Launcher class for Elinks browsers."""

class Konqueror(BaseBrowser):
    """Controller for the KDE File Manager (kfm, or Konqueror).

    See the output of ``kfmclient --commands``
    for more information on the Konqueror remote-control interface.
    """

    def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

if sys.platform == "win32":
    class WindowsDefault(BaseBrowser):
        def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

if sys.platform == "darwin":
    if sys.version_info < (3, 13):
        if sys.version_info >= (3, 11):
            @deprecated("Deprecated since Python 3.11; removed in Python 3.13.")
            class MacOSX(BaseBrowser):
                """Launcher class for Aqua browsers on Mac OS X

                Optionally specify a browser name on instantiation.  Note that this
                will not work for Aqua browsers if the user has moved the application
                package after installation.

                If no browser is specified, the default browser, as specified in the
                Internet System Preferences panel, will be used.
                """

                def __init__(self, name: str) -> None: ...
                def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

        else:
            class MacOSX(BaseBrowser):
                """Launcher class for Aqua browsers on Mac OS X

                Optionally specify a browser name on instantiation.  Note that this
                will not work for Aqua browsers if the user has moved the application
                package after installation.

                If no browser is specified, the default browser, as specified in the
                Internet System Preferences panel, will be used.
                """

                def __init__(self, name: str) -> None: ...
                def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...

    class MacOSXOSAScript(BaseBrowser):  # In runtime this class does not have `name` and `basename`
        if sys.version_info >= (3, 11):
            def __init__(self, name: str = "default") -> None: ...
        else:
            def __init__(self, name: str) -> None: ...

        def open(self, url: str, new: int = 0, autoraise: bool = True) -> bool: ...
