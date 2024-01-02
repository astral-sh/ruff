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


# Test that a doctest on the real last line of a docstring reformats
# correctly.
def doctest_really_last_line():
    """
    Do cool stuff.

    >>> cool_stuff( x )"""
    pass


# Test that a continued doctest on the real last line of a docstring reformats
# correctly.
def doctest_really_last_line_continued():
    """
    Do cool stuff.

    >>> cool_stuff( x )
    ... more( y )"""
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
# theory compute the indentation length of a code snippet and then
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
    # This demonstrates a normal line that will get wrapped but won't
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


###############################################################################
# reStructuredText CODE EXAMPLES
#
# This section shows examples of docstrings that contain code snippets in
# reStructuredText formatted code blocks.
#
# See: https://www.sphinx-doc.org/en/master/usage/restructuredtext/basics.html#literal-blocks
# See: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-code-block
# See: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#literal-blocks
# See: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#toc-entry-30
# See: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#toc-entry-38
###############################################################################


def rst_literal_simple():
    """
    Do cool stuff::

        cool_stuff( 1 )

    Done.
    """
    pass


def rst_literal_simple_continued():
    """
    Do cool stuff::

        def cool_stuff( x ):
            print( f"hi {x}" );

    Done.
    """
    pass


# Tests that we can end the literal block on the second
# to last line of the docstring.
def rst_literal_second_to_last():
    """
    Do cool stuff::

        cool_stuff( 1 )
    """
    pass


# Tests that we can end the literal block on the actual
# last line of the docstring.
def rst_literal_actually_last():
    """
    Do cool stuff::

        cool_stuff( 1 )"""
    pass


def rst_literal_with_blank_lines():
    """
    Do cool stuff::

        def cool_stuff( x ):
            print( f"hi {x}" );

        def other_stuff( y ):
            print(    y     )

    Done.
    """
    pass


# Extra blanks should be preserved.
def rst_literal_extra_blanks():
    """
    Do cool stuff::



        cool_stuff( 1 )



    Done.
    """
    pass


# If a literal block is never properly ended (via a non-empty unindented line),
# then the end of the block should be the last non-empty line. And subsequent
# empty lines should be preserved as-is.
def rst_literal_extra_blanks_at_end():
    """
    Do cool stuff::


        cool_stuff( 1 )



    """
    pass


# A literal block can contain many empty lines and it should not end the block
# if it continues.
def rst_literal_extra_blanks_in_snippet():
    """
    Do cool stuff::

        cool_stuff( 1 )


        cool_stuff( 2 )

    Done.
    """
    pass


# This tests that a unindented line appearing after an indented line (but where
# the indent is still beyond the minimum) gets formatted properly.
def rst_literal_subsequent_line_not_indented():
    """
    Do cool stuff::

     if True:
        cool_stuff( '''
     hiya''' )

    Done.
    """
    pass


# This checks that if the first line in a code snippet has been indented with
# tabs, then so long as its "indentation length" is considered bigger than the
# line with `::`, it is reformatted as code.
#
# (If your tabwidth is set to 4, then it looks like the code snippet
# isn't indented at all, which is perhaps counter-intuitive. Indeed, reST
# itself also seems to recognize this as a code block, although it appears
# under-specified.)
def rst_literal_first_line_indent_uses_tabs_4spaces():
    """
    Do cool stuff::

	cool_stuff( 1 )

    Done.
    """
    pass


# Like the test above, but with multiple lines.
def rst_literal_first_line_indent_uses_tabs_4spaces_multiple():
    """
    Do cool stuff::

	cool_stuff( 1 )
	cool_stuff( 2 )

    Done.
    """
    pass


# Another test with tabs, except in this case, if your tabwidth is less than
# 8, than the code snippet actually looks like its indent is *less* than the
# opening line with a `::`. One might presume this means that the code snippet
# is not treated as a literal block and thus not reformatted, but since we
# assume all tabs have tabwidth=8 when computing indentation length, the code
# snippet is actually seen as being more indented than the opening `::` line.
# As with the above example, reST seems to behave the same way here.
def rst_literal_first_line_indent_uses_tabs_8spaces():
        """
        Do cool stuff::

	 cool_stuff( 1 )

        Done.
        """
        pass


# Like the test above, but with multiple lines.
def rst_literal_first_line_indent_uses_tabs_8spaces_multiple():
        """
        Do cool stuff::

	 cool_stuff( 1 )
	 cool_stuff( 2 )

        Done.
        """
        pass


# Tests that if two lines in a literal block are indented to the same level
# but by different means (tabs versus spaces), then we correctly recognize the
# block and format it.
def rst_literal_first_line_tab_second_line_spaces():
    """
    Do cool stuff::

	cool_stuff( 1 )
        cool_stuff( 2 )

    Done.
    """
    pass


# Tests that when two lines in a code snippet have weird and inconsistent
# indentation, the code still gets formatted so long as the indent is greater
# than the indent of the `::` line.
#
# In this case, the minimum indent is 5 spaces (from the second line) where as
# the first line has an indent of 8 spaces via a tab (by assuming tabwidth=8).
# The minimum indent is stripped from each code line. Since tabs aren't
# divisible, the entire tab is stripped, which means the first and second lines
# wind up with the same level of indentation.
#
# An alternative behavior here would be that the tab is replaced with 3 spaces
# instead of being stripped entirely. The code snippet itself would then have
# inconsistent indentation to the point of being invalid Python, and thus code
# formatting would be skipped.
#
# I decided on the former behavior because it seems a bit easier to implement,
# but we might want to switch to the alternative if cases like this show up in
# the real world. ---AG
def rst_literal_odd_indentation():
    """
    Do cool stuff::

	cool_stuff( 1 )
     cool_stuff( 2 )

    Done.
    """
    pass


# Tests that having a line with a lone `::` works as an introduction of a
# literal block.
def rst_literal_lone_colon():
    """
    Do cool stuff.

    ::

        cool_stuff( 1 )

    Done.
    """
    pass


def rst_directive_simple():
    """
    .. code-block:: python

        cool_stuff( 1 )

    Done.
    """
    pass


def rst_directive_case_insensitive():
    """
    .. cOdE-bLoCk:: python

        cool_stuff( 1 )

    Done.
    """
    pass


def rst_directive_sourcecode():
    """
    .. sourcecode:: python

        cool_stuff( 1 )

    Done.
    """
    pass


def rst_directive_options():
    """
    .. code-block:: python
        :linenos:
        :emphasize-lines: 2,3
        :name: blah blah

        cool_stuff( 1 )
        cool_stuff( 2 )
        cool_stuff( 3 )
        cool_stuff( 4 )

    Done.
    """
    pass


# In this case, since `pycon` isn't recognized as a Python code snippet, the
# docstring reformatter ignores it. But it then picks up the doctest and
# reformats it.
def rst_directive_doctest():
    """
    .. code-block:: pycon

        >>> cool_stuff( 1 )

    Done.
    """
    pass


# This checks that if the first non-empty line after the start of a literal
# block is not indented more than the line containing the `::`, then it is not
# treated as a code snippet.
def rst_literal_skipped_first_line_not_indented():
    """
    Do cool stuff::

    cool_stuff( 1 )

    Done.
    """
    pass


# Like the test above, but inserts an indented line after the un-indented one.
# This should not cause the literal block to be resumed.
def rst_literal_skipped_first_line_not_indented_then_indented():
    """
    Do cool stuff::

    cool_stuff( 1 )
      cool_stuff( 2 )

    Done.
    """
    pass


# This also checks that a code snippet is not reformatted when the indentation
# of the first line is not more than the line with `::`, but this uses tabs to
# make it a little more confounding. It relies on the fact that indentation
# length is computed by assuming a tabwidth equal to 8. reST also rejects this
# and doesn't treat it as a literal block.
def rst_literal_skipped_first_line_not_indented_tab():
        """
        Do cool stuff::

	cool_stuff( 1 )

        Done.
        """
        pass


# Like the previous test, but adds a second line.
def rst_literal_skipped_first_line_not_indented_tab_multiple():
        """
        Do cool stuff::

	cool_stuff( 1 )
	cool_stuff( 2 )

        Done.
        """
        pass


# Tests that a code block with a second line that is not properly indented gets
# skipped. A valid code block needs to have an empty line separating these.
#
# One trick here is that we need to make sure the Python code in the snippet is
# valid, otherwise it would be skipped because of invalid Python.
def rst_literal_skipped_subsequent_line_not_indented():
    """
    Do cool stuff::

     if True:
        cool_stuff( '''
    hiya''' )

    Done.
    """
    pass


# In this test, we write what looks like a code-block, but it should be treated
# as invalid due to the missing `language` argument.
#
# It does still look like it could be a literal block according to the literal
# rules, but we currently consider the `.. ` prefix to indicate that it is not
# a literal block.
def rst_literal_skipped_not_directive():
    """
    .. code-block::

        cool_stuff( 1 )

    Done.
    """
    pass


# In this test, we start a line with `.. `, which makes it look like it might
# be a directive. But instead continue it as if it was just some periods from
# the previous line, and then try to end it by starting a literal block.
#
# But because of the `.. ` in the beginning, we wind up not treating this as a
# code snippet. The reST render I was using to test things does actually treat
# this as a code block, so we may be out of conformance here.
def rst_literal_skipped_possible_false_negative():
    """
    This is a test.
    .. This is a test::

        cool_stuff( 1 )

    Done.
    """
    pass


# This tests that a doctest inside of a reST literal block doesn't get
# reformatted. It's plausible this isn't the right behavior, but it also seems
# like it might be the right behavior since it is a literal block. (The doctest
# makes the Python code invalid.)
def rst_literal_skipped_doctest():
    """
    Do cool stuff::

        >>> cool_stuff( 1 )

    Done.
    """
    pass


def rst_literal_skipped_markdown():
    """
    Do cool stuff::

        ```py
        cool_stuff( 1 )
        ```

    Done.
    """
    pass


def rst_directive_skipped_not_indented():
    """
    .. code-block:: python

    cool_stuff( 1 )

    Done.
    """
    pass


def rst_directive_skipped_wrong_language():
    """
    .. code-block:: rust

        cool_stuff( 1 )

    Done.
    """
    pass


# This gets skipped for the same reason that the doctest in a literal block
# gets skipped.
def rst_directive_skipped_doctest():
    """
    .. code-block:: python

        >>> cool_stuff( 1 )

    Done.
    """
    pass


###############################################################################
# Markdown CODE EXAMPLES
#
# This section shows examples of docstrings that contain code snippets in
# Markdown fenced code blocks.
#
# See: https://spec.commonmark.org/0.30/#fenced-code-blocks
###############################################################################


def markdown_simple():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    ```

    Done.
    """
    pass


def markdown_simple_continued():
    """
    Do cool stuff.

    ```python
    def cool_stuff( x ):
        print( f"hi {x}" );
    ```

    Done.
    """
    pass


# Tests that unlabeled Markdown fenced code blocks are assumed to be Python.
def markdown_unlabeled():
    """
    Do cool stuff.

    ```
    cool_stuff( 1 )
    ```

    Done.
    """
    pass


# Tests that fenced code blocks using tildes work.
def markdown_tildes():
    """
    Do cool stuff.

    ~~~py
    cool_stuff( 1 )
    ~~~

    Done.
    """
    pass


# Tests that a longer closing fence is just fine and dandy.
def markdown_longer_closing_fence():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    ``````

    Done.
    """
    pass


# Tests that an invalid closing fence is treated as invalid.
#
# We embed it into a docstring so that the surrounding Python
# remains valid.
def markdown_longer_closing_fence():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    '''
    ```invalid
    '''
    cool_stuff( 2 )
    ```

    Done.
    """
    pass


# Tests that one can nest fenced code blocks by using different numbers of
# backticks.
def markdown_nested_fences():
    """
    Do cool stuff.

    ``````
    do_something( '''
    ```
    did i trick you?
    ```
    ''' )
    ``````

    Done.
    """
    pass


# Tests that an unclosed block gobbles up everything remaining in the
# docstring. When it's only empty lines, those are passed into the formatter
# and thus stripped.
def markdown_unclosed_empty_lines():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )



    """
    pass


# Tests that we can end the block on the second to last line of the
# docstring.
def markdown_second_to_last():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    ```
    """
    pass


# Tests that an unclosed block with one extra line at the end is treated
# correctly. As per the CommonMark spec, an unclosed fenced code block contains
# everything following the opening fences. Since formatting the code snippet
# trims lines, the last empty line is removed here.
def markdown_second_to_last():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    """
    pass


# Tests that we can end the block on the actual last line of the docstring.
def markdown_actually_last():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    ```"""
    pass


# Tests that an unclosed block that ends on the last line of a docstring
# is handled correctly.
def markdown_unclosed_actually_last():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )"""
    pass


def markdown_with_blank_lines():
    """
    Do cool stuff.

    ```py
    def cool_stuff( x ):
        print( f"hi {x}" );

    def other_stuff( y ):
        print(    y     )
    ```

    Done.
    """
    pass


def markdown_first_line_indent_uses_tabs_4spaces():
    """
    Do cool stuff.

    ```py
	cool_stuff( 1 )
    ```

    Done.
    """
    pass


def markdown_first_line_indent_uses_tabs_4spaces_multiple():
    """
    Do cool stuff.

    ```py
	cool_stuff( 1 )
	cool_stuff( 2 )
    ```

    Done.
    """
    pass


def markdown_first_line_indent_uses_tabs_8spaces():
        """
        Do cool stuff.

	 ```py
	 cool_stuff( 1 )
	 ```

        Done.
        """
        pass


def markdown_first_line_indent_uses_tabs_8spaces_multiple():
        """
        Do cool stuff.

	 ```py
	 cool_stuff( 1 )
	 cool_stuff( 2 )
	 ```

        Done.
        """
        pass


def markdown_first_line_tab_second_line_spaces():
    """
    Do cool stuff.

	```py
	cool_stuff( 1 )
        cool_stuff( 2 )
	```

    Done.
    """
    pass


def markdown_odd_indentation():
    """
    Do cool stuff.

	```py
	cool_stuff( 1 )
        cool_stuff( 2 )
	```

    Done.
    """
    pass


# Extra blanks should be *not* be preserved (unlike reST) because they are part
# of the code snippet (per CommonMark spec), and thus get trimmed as part of
# code formatting.
def markdown_extra_blanks():
    """
    Do cool stuff.

    ```py


    cool_stuff( 1 )


    ```

    Done.
    """
    pass


# A block can contain many empty lines within it.
def markdown_extra_blanks_in_snippet():
    """
    Do cool stuff.

    ```py

    cool_stuff( 1 )


    cool_stuff( 2 )
    ```

    Done.
    """
    pass


def markdown_weird_closing():
    """
  Code block with weirdly placed closing fences.

    ```python
    cool_stuff( 1 )

         ```
      # The above fences look like it shouldn't close the block, but we
      # allow it to. The fences below re-open a block (until the end of
      # the docstring), but it's invalid Python and thus doesn't get
      # reformatted.
        a = 10
    ```

    Now the code block is closed
    """
    pass


def markdown_over_indented():
    """
    A docstring
        over intended
        ```python
        print( 5 )
        ```
    """
    pass


# This tests that we can have additional text after the language specifier.
def markdown_additional_info_string():
    """
    Do cool stuff.

    ```python tab="plugin.py"
    cool_stuff( 1 )
    ```

    Done.
    """
    pass


# Tests that an unclosed block gobbles up everything remaining in the
# docstring, even if it isn't valid Python. Since it isn't valid Python,
# reformatting fails and the entire thing is skipped.
def markdown_skipped_unclosed_non_python():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )

    I forgot to close the code block, and this is definitely not
    Python. So nothing here gets formatted.
    """
    pass


# This has a Python snippet with a docstring that contains a closing fence.
# This splits the embedded docstring and makes the overall snippet invalid.
def markdown_skipped_accidental_closure():
    """
    Do cool stuff.

    ```py
    cool_stuff( 1 )
    '''
    ```
    '''
    ```

    Done.
    """
    pass


# When a line is unindented all the way out before the standard indent of the
# docstring, the code reformatting ends up interacting poorly with the standard
# docstring whitespace normalization logic. This is probably a bug, and we
# should probably treat the Markdown block as valid, but for now, we detect
# the unindented line and declare the block as invalid and thus do no code
# reformatting.
#
# FIXME: Fixing this (if we think it's a bug) probably requires refactoring the
# docstring whitespace normalization to be aware of code snippets. Or perhaps
# plausibly, to do normalization *after* code snippets have been formatted.
def markdown_skipped_unindented_completely():
    """
    Do cool stuff.

        ```py
cool_stuff( 1 )
        ```

    Done.
    """
    pass


# This test is fallout from treating fenced code blocks with unindented lines
# as invalid. We probably should treat this as a valid block. Indeed, if we
# remove the logic that makes the `markdown_skipped_unindented_completely` test
# pass, then this code snippet will get reformatted correctly.
def markdown_skipped_unindented_somewhat():
    """
    Do cool stuff.

        ```py
    cool_stuff( 1 )
        ```

    Done.
    """
    pass


# This tests that if a Markdown block contains a line that has less of an
# indent than another line.
#
# There is some judgment involved in what the right behavior is here. We
# could "normalize" the indentation so that the minimum is the indent of the
# opening fence line. If we did that here, then the code snippet would become
# valid and format as Python. But at time of writing, we don't, which leads to
# inconsistent indentation and thus invalid Python.
def markdown_skipped_unindented_with_inconsistent_indentation():
    """
    Do cool stuff.

        ```py
    cool_stuff( 1 )
        cool_stuff( 2 )
        ```

    Done.
    """
    pass


def markdown_skipped_doctest():
    """
    Do cool stuff.

    ```py
    >>> cool_stuff( 1 )
    ```

    Done.
    """
    pass


def markdown_skipped_rst_literal():
    """
    Do cool stuff.

    ```py
    And do this::

        cool_stuff( 1 )

    ```

    Done.
    """
    pass


def markdown_skipped_rst_directive():
    """
    Do cool stuff.

    ```py
    .. code-block:: python

        cool_stuff( 1 )

    ```

    Done.
    """
    pass
