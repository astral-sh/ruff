def a1():
    """Needs chaperone\\" """

def a2():
    """Needs chaperone\\\ """

def a3():
    """Needs chaperone" """

def a4():
    """Needs chaperone\ """

def a5():
    """Needs chaperone\\\\\ """

def a6():
    """Needs chaperone\"" """

def a7():
    """Doesn't need chaperone\""""

def a8():
    """Doesn't need chaperone\'"""

def a9():
    """Doesn't need chaperone\\\""""

def a10():
    """Doesn't need chaperone\\\'"""

def a11():
    """Doesn't need chaperone; all slashes escaped\\\\"""

def a12():
    """Doesn't need chaperone\\"""

def a12():
    """Doesn't need "chaperone" with contained quotes"""

def a13():
    """Doesn't need chaperone\\\"\"\""""

def a14():
    """Multiline docstring
    doesn't need chaperone
    """

def a15():
    """Multiline docstring
    doesn't need chaperone\
    """

def a16():
    """Multiline docstring with
    odd number of backslashes don't need chaperone\\\
    """

def a17():
    """Multiline docstring with
    even number of backslashes don't need chaperone\\\\
    """

def a18():
    r"""Raw multiline docstring
    doesn't need chaperone\
    """
