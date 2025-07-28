"""A ScrolledText widget feels like a text widget but also has a
vertical scroll bar on its right.  (Later, options may be added to
add a horizontal bar as well, to make the bars disappear
automatically when not needed, to move them to the other side of the
window, etc.)

Configuration options are passed to the Text widget.
A Frame widget is inserted between the master and the text, to hold
the Scrollbar widget.
Most methods calls are inherited from the Text widget; Pack, Grid and
Place methods are redirected to the Frame widget however.
"""

from tkinter import Frame, Misc, Scrollbar, Text

__all__ = ["ScrolledText"]

# The methods from Pack, Place, and Grid are dynamically added over the parent's impls
class ScrolledText(Text):
    frame: Frame
    vbar: Scrollbar
    def __init__(self, master: Misc | None = None, **kwargs) -> None: ...
