"""A one line summary of the module or program, terminated by a period.

Leave one blank line.  The rest of this docstring should contain an
overall description of the module or program.  Optionally, it may also
contain a brief description of exported classes and functions and/or usage
examples.

  Typical usage example:

  foo = ClassFoo()
  bar = foo.FunctionBar()
"""
# above: "2.8.2 Modules" section example
# https://google.github.io/styleguide/pyguide.html#382-modules

# Examples from the official "Google Python Style Guide" documentation:
#     * As HTML: https://google.github.io/styleguide/pyguide.html
#     * Source Markdown:
#         https://github.com/google/styleguide/blob/gh-pages/pyguide.md

import os
from .expected import Expectation

expectation = Expectation()
expect = expectation.expect

# module docstring expected violations:
expectation.expected.add((
    os.path.normcase(__file__),
    "D213: Multi-line docstring summary should start at the second line"))


# "3.8.3 Functions and Methods" section example
# https://google.github.io/styleguide/pyguide.html#383-functions-and-methods
@expect("D213: Multi-line docstring summary should start at the second line",
        arg_count=3)
@expect("D401: First line should be in imperative mood "
        "(perhaps 'Fetch', not 'Fetches')", arg_count=3)
@expect("D406: Section name should end with a newline "
        "('Raises', not 'Raises:')", arg_count=3)
@expect("D406: Section name should end with a newline "
        "('Returns', not 'Returns:')", arg_count=3)
@expect("D407: Missing dashed underline after section ('Raises')", arg_count=3)
@expect("D407: Missing dashed underline after section ('Returns')",
        arg_count=3)
@expect("D413: Missing blank line after last section ('Raises')", arg_count=3)
def fetch_bigtable_rows(big_table, keys, other_silly_variable=None):
    """Fetches rows from a Bigtable.

    Retrieves rows pertaining to the given keys from the Table instance
    represented by big_table.  Silly things may happen if
    other_silly_variable is not None.

    Args:
        big_table: An open Bigtable Table instance.
        keys: A sequence of strings representing the key of each table row
            to fetch.
        other_silly_variable: Another optional variable, that has a much
            longer name than the other args, and which does nothing.

    Returns:
        A dict mapping keys to the corresponding table row data
        fetched. Each row is represented as a tuple of strings. For
        example:

        {'Serak': ('Rigel VII', 'Preparer'),
         'Zim': ('Irk', 'Invader'),
         'Lrrr': ('Omicron Persei 8', 'Emperor')}

        If a key from the keys argument is missing from the dictionary,
        then that row was not found in the table.

    Raises:
        IOError: An error occurred accessing the bigtable.Table object.
    """


# "3.8.4 Classes" section example
# https://google.github.io/styleguide/pyguide.html#384-classes
@expect("D203: 1 blank line required before class docstring (found 0)")
@expect("D213: Multi-line docstring summary should start at the second line")
@expect("D406: Section name should end with a newline "
        "('Attributes', not 'Attributes:')")
@expect("D407: Missing dashed underline after section ('Attributes')")
@expect("D413: Missing blank line after last section ('Attributes')")
class SampleClass:
    """Summary of class here.

    Longer class information....
    Longer class information....

    Attributes:
        likes_spam: A boolean indicating if we like SPAM or not.
        eggs: An integer count of the eggs we have laid.
    """

    @expect("D401: First line should be in imperative mood "
            "(perhaps 'Init', not 'Inits')", arg_count=2)
    def __init__(self, likes_spam=False):
        """Inits SampleClass with blah."""
        if self:  # added to avoid NameError when run via @expect decorator
            self.likes_spam = likes_spam
            self.eggs = 0

    @expect("D401: First line should be in imperative mood "
            "(perhaps 'Perform', not 'Performs')", arg_count=1)
    def public_method(self):
        """Performs operation blah."""
