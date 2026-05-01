"""A module-level docstring with a Sphinx directive containing section-like content.

.. code-block:: yaml

    references:
      - ref: Bibliographic citation in your favorite format.
        refType: open literature

This is more text after the directive.
"""


def func():
    """A function-level docstring with a Sphinx directive.

    Examples:
        This is an example.

    .. code-block:: python

        returns = "not a section"
        notes = "also not a section"

    Returns:
        None
    """


def func2():
    """A function-level docstring with single-colon directive (invalid RST but still common).

    .. code-block: yaml

        references:
          - ref: Some reference.

    More text.
    """


def func3():
    """A function-level docstring with nested directives.

    .. note::

        .. code-block:: python

            warnings = "not a section"

    Returns:
        None
    """


def func4():
    """A function-level docstring where a real section follows a directive.

    .. code-block:: python

        example = "code"

    Notes:
        This IS a real section and should still be detected.
    """
