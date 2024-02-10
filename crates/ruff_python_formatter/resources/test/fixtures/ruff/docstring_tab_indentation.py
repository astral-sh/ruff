# Tests the behavior of the formatter when it comes to tabs inside docstrings
# when using `indent_style="tab`

# The example below uses tabs exclusively. The formatter should preserve the tab indentation
# of `arg1`.
def tab_argument(arg1: str) -> None:
	"""
	Arguments:
		arg1: super duper arg with 2 tabs in front
	"""

# The `arg1` is intended with spaces. The formatter should not change the spaces to a tab
# because it must assume that the spaces are used for alignment and not indentation.
def space_argument(arg1: str) -> None:
	"""
	Arguments:
	        arg1: super duper arg with a tab and a space in front
	"""

def under_indented(arg1: str) -> None:
	"""
	Arguments:
	        arg1: super duper arg with a tab and a space in front
arg2: Not properly indented
	"""

def under_indented_tabs(arg1: str) -> None:
	"""
	Arguments:
		arg1: super duper arg with a tab and a space in front
arg2: Not properly indented
	"""

def spaces_tabs_over_indent(arg1: str) -> None:
    """
    Arguments:
      	arg1: super duper arg with a tab and a space in front
    """

# The docstring itself is indented with spaces but the argument is indented by a tab.
# Keep the tab indentation of the argument, convert th docstring indent to tabs.
def space_indented_docstring_containing_tabs(arg1: str) -> None:
    """
    Arguments:
    	arg1: super duper arg
    """


# The docstring uses tabs, spaces, tabs indentation.
# Fallback to use space indentation
def mixed_indentation(arg1: str) -> None:
	"""
	Arguments:
	        	arg1: super duper arg with a tab and a space in front
	"""


# The example shows an ascii art. The formatter should not change the spaces
# to tabs because it breaks the ASCII art when inspecting the docstring with `inspect.cleandoc(ascii_art.__doc__)`
# when using an indent width other than 8.
def ascii_art():
	r"""
	Look at this beautiful tree.

	    a
	   / \
	  b   c
	 / \
	d   e
	"""


