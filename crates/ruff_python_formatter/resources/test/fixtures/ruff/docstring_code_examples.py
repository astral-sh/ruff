###############################################################################
# DOCTEST CODE EXAMPLES
#
# This section shows examples of docstrings that contain code snippets in
# Python's "doctest" format.
#
# See: https://docs.python.org/3/library/doctest.html
###############################################################################

# The simplest doctest to ensure basic formatting works.
def doctest_simple():
    """
    Do cool stuff.

    >>> cool_stuff( 1 )
    2
    """
    pass


# Another simple test, but one where the Python code
# extends over multiple lines.
def doctest_simple_continued():
    """
    Do cool stuff.

    >>> def cool_stuff( x ):
    ...     print( f"hi {x}" );
    hi 2
    """
    pass


# Test that we support multiple directly adjacent
# doctests.
def doctest_adjacent():
    """
    Do cool stuff.

    >>> cool_stuff( x )
    >>> cool_stuff( y )
    2
    """
    pass


# Test that a doctest on the last non-whitespace line of a docstring
# reformats correctly.
def doctest_last_line():
    """
    Do cool stuff.

    >>> cool_stuff( x )
    """
    pass


# Test that a doctest that continues to the last non-whitespace line of
# a docstring reformats correctly.
def doctest_last_line_continued():
    """
    Do cool stuff.

    >>> def cool_stuff( x ):
    ...     print( f"hi {x}" );
    """
    pass


# Test that a doctest is correctly identified and formatted with a blank
# continuation line.
def doctest_blank_continued():
    """
    Do cool stuff.

    >>> def cool_stuff ( x ):
    ...     print( x )
    ...
    ...     print( x )
    """
    pass


# Tests that a blank PS2 line at the end of a doctest can get dropped.
# It is treated as part of the Python snippet which will trim the
# trailing whitespace.
def doctest_blank_end():
    """
    Do cool stuff.

    >>> def cool_stuff ( x ):
    ...     print( x )
    ...     print( x )
    ...
    """
    pass


# Tests that a blank PS2 line at the end of a doctest can get dropped
# even when there is text following it.
def doctest_blank_end_then_some_text():
    """
    Do cool stuff.

    >>> def cool_stuff ( x ):
    ...     print( x )
    ...     print( x )
    ...

    And say something else.
    """
    pass


# Test that a doctest containing a triple quoted string gets formatted
# correctly and doesn't result in invalid syntax.
def doctest_with_triple_single():
    """
    Do cool stuff.

    >>> x        =      '''tricksy'''
    """
    pass


# Test that a doctest containing a triple quoted f-string gets
# formatted correctly and doesn't result in invalid syntax.
def doctest_with_triple_single():
    """
    Do cool stuff.

    >>> x    = f'''tricksy'''
    """
    pass


# Another nested multi-line string case, but with triple escaped double
# quotes inside a triple single quoted string.
def doctest_with_triple_escaped_double():
    """
    Do cool stuff.

    >>> x       =       '''\"\"\"'''
    """
    pass


# Tests that inverting the triple quoting works as expected.
def doctest_with_triple_inverted():
    '''
    Do cool stuff.

    >>> x   =   """tricksy"""
    '''
    pass


# Tests that inverting the triple quoting with an f-string works as
# expected.
def doctest_with_triple_inverted_fstring():
    '''
    Do cool stuff.

    >>> x        = f"""tricksy"""
    '''
    pass


# Tests nested doctests are ignored. That is, we don't format doctests
# recursively. We only recognize "top level" doctests.
#
# This restriction primarily exists to avoid needing to deal with
# nesting quotes. It also seems like a generally sensible restriction,
# although it could be lifted if necessary I believe.
def doctest_nested_doctest_not_formatted():
    '''
    Do cool stuff.

    >>> def nested( x   ):
    ...     """
    ...     Do nested cool stuff.
    ...     >>> func_call( 5 )
    ...     """
    ...     pass
    '''
    pass


# Tests that the starting column does not matter.
def doctest_varying_start_column():
    '''
    Do cool stuff.

    >>> assert    ("Easy!")
      >>> import                      math
          >>> math.floor(  1.9  )
          1
    '''
    pass


# Tests that long lines get wrapped... appropriately.
#
# The docstring code formatter uses the same line width settings as for
# formatting other code. This means that a line in the docstring can
# actually extend past the configured line limit.
#
# It's not quite clear whether this is desirable or not. We could in
# theory compute the intendation length of a code snippet and then
# adjust the line-width setting on a recursive call to the formatter.
# But there are assuredly pathological cases to consider. Another path
# would be to expose another formatter option for controlling the
# line-width of code snippets independently.
def doctest_long_lines():
    '''
    Do cool stuff.

    This won't get wrapped even though it exceeds our configured
    line width because it doesn't exceed the line width within this
    docstring. e.g, the `f` in `foo` is treated as the first column.
    >>> foo, bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)

    But this one is long enough to get wrapped.
    >>> foo, bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey, spider, bear, leopard)
    '''
    # This demostrates a normal line that will get wrapped but won't
    # get wrapped in the docstring above because of how the line-width
    # setting gets reset at the first column in each code snippet.
    foo, bar, quux = this_is_a_long_line(lion, giraffe, hippo, zeba, lemur, penguin, monkey)


# Checks that a simple but invalid doctest gets skipped.
def doctest_skipped_simple():
    """
    Do cool stuff.

    >>> cool-stuff( x ):
    2
    """
    pass


# Checks that a simple doctest that is continued over multiple lines,
# but is invalid, gets skipped.
def doctest_skipped_simple_continued():
    """
    Do cool stuff.

    >>> def cool-stuff( x ):
    ...     print( f"hi {x}" );
    2
    """
    pass


# Checks that a doctest with improper indentation gets skipped.
def doctest_skipped_inconsistent_indent():
    """
    Do cool stuff.

     >>> def cool_stuff( x ):
    ...     print( f"hi {x}" );
    hi 2
    """
    pass

# Checks that a doctest with some proper indentation and some improper
# indentation is "partially" formatted. That is, the part that appears
# before the inconsistent indentation is formatted. This requires that
# the part before it is valid Python.
def doctest_skipped_partial_inconsistent_indent():
    """
    Do cool stuff.

     >>> def cool_stuff( x ):
     ...     print( x )
    ...     print( f"hi {x}" );
    hi 2
    """
    pass


# Checks that a doctest with improper triple single quoted string gets
# skipped. That is, the code snippet is itself invalid Python, so it is
# left as is.
def doctest_skipped_triple_incorrect():
    """
    Do cool stuff.

    >>> foo( x )
    ... '''tri'''cksy'''
    """
    pass


# Tests that a doctest on a single line is skipped.
def doctest_skipped_one_line():
    ">>> foo( x )"
    pass


# f-strings are not considered docstrings[1], so any doctests
# inside of them should not be formatted.
#
# [1]: https://docs.python.org/3/reference/lexical_analysis.html#formatted-string-literals
def doctest_skipped_fstring():
    f"""
    Do cool stuff.

    >>> cool_stuff( 1 )
    2
    """
    pass


# Test that a doctest containing a triple quoted string at least
# does not result in invalid Python code. Ideally this would format
# correctly, but at time of writing it does not.
def doctest_invalid_skipped_with_triple_double_in_single_quote_string():
    """
    Do cool stuff.

    >>> x        =      '\"\"\"'
    """
    pass
