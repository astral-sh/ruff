from _curses import window
from typing import Final, final

__version__: Final[str]
version: Final[str]

class error(Exception): ...

@final
class panel:
    def above(self) -> panel:
        """Return the panel above the current panel."""

    def below(self) -> panel:
        """Return the panel below the current panel."""

    def bottom(self) -> None:
        """Push the panel to the bottom of the stack."""

    def hidden(self) -> bool:
        """Return True if the panel is hidden (not visible), False otherwise."""

    def hide(self) -> None:
        """Hide the panel.

        This does not delete the object, it just makes the window on screen invisible.
        """

    def move(self, y: int, x: int, /) -> None:
        """Move the panel to the screen coordinates (y, x)."""

    def replace(self, win: window, /) -> None:
        """Change the window associated with the panel to the window win."""

    def set_userptr(self, obj: object, /) -> None:
        """Set the panel's user pointer to obj."""

    def show(self) -> None:
        """Display the panel (which might have been hidden)."""

    def top(self) -> None:
        """Push panel to the top of the stack."""

    def userptr(self) -> object:
        """Return the user pointer for the panel."""

    def window(self) -> window:
        """Return the window object associated with the panel."""

def bottom_panel() -> panel:
    """Return the bottom panel in the panel stack."""

def new_panel(win: window, /) -> panel:
    """Return a panel object, associating it with the given window win."""

def top_panel() -> panel:
    """Return the top panel in the panel stack."""

def update_panels() -> panel:
    """Updates the virtual screen after changes in the panel stack.

    This does not call curses.doupdate(), so you'll have to do this yourself.
    """
