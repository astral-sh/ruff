#!/usr/bin/env python3
# fmt: on
# Some license here.
#
# Has many lines. Many, many lines.
# Many, many, many lines.
"""Module docstring.

Possibly also many, many lines.
"""

import os.path
import sys

import a
from b.c import X  # some noqa comment

try:
    import fast
except ImportError:
    import slow as fast


# Some comment before a function.
y = 1
(
    # some strings
    y  # type: ignore
)


def function(default=None):
    """Docstring comes first.

    Possibly many lines.
    """
    # FIXME: Some comment about why this function is crap but still in production.
    import inner_imports

    if inner_imports.are_evil():
        # Explains why we have this if.
        # In great detail indeed.
        x = X()
        return x.method1()  # type: ignore

    # This return is also commented for some reason.
    return default


# Explains why we use global state.
GLOBAL_STATE = {"a": a(1), "b": a(2), "c": a(3)}


# Another comment!
# This time two lines.


class Foo:
    """Docstring for class Foo.  Example from Sphinx docs."""

    #: Doc comment for class attribute Foo.bar.
    #: It can have multiple lines.
    bar = 1

    flox = 1.5  #: Doc comment for Foo.flox. One line only.

    baz = 2
    """Docstring for class attribute Foo.baz."""

    def __init__(self):
        #: Doc comment for instance attribute qux.
        self.qux = 3

        self.spam = 4
        """Docstring for instance attribute spam."""


#' <h1>This is pweave!</h1>


@fast(really=True)
async def wat():
    # This comment, for some reason \
    # contains a trailing backslash.
    async with X.open_async() as x:  # Some more comments
        result = await x.method1()
    # Comment after ending a block.
    if result:
        print("A OK", file=sys.stdout)
        # Comment between things.
        print()


# Some closing comments.
# Maybe Vim or Emacs directives for formatting.
# Who knows.
