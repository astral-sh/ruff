def single_line_backslashes1():
  """ content\     """
  return


def single_line_backslashes2():
  """ content\\     """
  return


def single_line_backslashes3():
  """ content\\\     """
  return


def multiline_backslashes1():
  """This is a docstring with
  some lines of text\     """
  return


def multiline_backslashes2():
  """This is a docstring with
  some lines of text\\     """
  return


def multiline_backslashes3():
  """This is a docstring with
  some lines of text\\\     """
  return


def multiple_negatively_indented_docstring_lines():
    """a
 b
  c
   d
    e
    """


def overindentend_docstring():
    """a
            over-indented
    """


def comment_before_docstring():
    # don't lose this function comment ...
    """Does nothing.

    But it has comments
    """  # ... neither lose this function comment


class CommentBeforeDocstring():
    # don't lose this class comment ...
    """Empty class.

    But it has comments
    """  # ... neither lose this class comment


class IndentMeSome:
    def doc_string_without_linebreak_after_colon(self): """ This is somewhat strange
         a
      b
         We format this a is the docstring had started properly indented on the next
         line if the target indentation. This may we incorrect since source and target
         indentation can be incorrect, but this is also an edge case.
         """


class IgnoreImplicitlyConcatenatedStrings:
    """""" ""


def docstring_that_ends_with_quote_and_a_line_break1():
    """
    he said "the news of my death have been greatly exaggerated"
    """


def docstring_that_ends_with_quote_and_a_line_break2():
    """he said "the news of my death have been greatly exaggerated"
    """


def docstring_that_ends_with_quote_and_a_line_break3():
    """he said "the news of my death have been greatly exaggerated"

    """


class TabbedIndent:
	def tabbed_indent(self):
		"""check for correct tabbed formatting
		                            ^^^^^^^^^^
		Normal indented line
		  	- autor
		"""
