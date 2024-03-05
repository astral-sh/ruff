# Tests that Ruff correctly preserves newlines before the closing quote

def test():
    """a, b
c"""


def test2():
    """a, b
    c
"""

def test3():
    """a, b

"""


def test4():
    """
    a, b

"""


def test5():
    """
    a, b
a"""

def test6():
    """
    a, b

    c
"""



def test7():
    """
    a, b




"""

def test7():
    """
    a, b



    """
