class Sample:
    """Needs chaperone\\" """

    """Needs chaperone\\\ """

    """Needs chaperone" """

    """Needs chaperone\ """

    """Needs chaperone\\\\\ """

    """Needs chaperone\"" """

    """Doesn't need chaperone\""""

    """Doesn't need chaperone\'"""

    """Doesn't need chaperone\\\""""

    """Doesn't need chaperone\\\'"""

    """Doesn't need chaperone; all slashes escaped\\\\"""

    """Doesn't need chaperone\\"""

    """Doesn't need "chaperone" with contained quotes"""

    """Doesn't need chaperone\\\"\"\""""

    """Multiline docstring
    doesn't need chaperone
    """

    """Multiline docstring
    doesn't need chaperone\
    """

    """Multiline docstring with
    odd number of backslashes don't need chaperone\\\
    """

    """Multiline docstring with
    even number of backslashes don't need chaperone\\\\
    """

    r"""Raw multiline docstring
    doesn't need chaperone\
    """

    def __init__(self):
        pass
