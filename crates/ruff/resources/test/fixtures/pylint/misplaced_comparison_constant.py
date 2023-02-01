"""Check that the constants are on the right side of the comparisons"""

# pylint: disable=singleton-comparison, missing-docstring, too-few-public-methods
# pylint: disable=comparison-of-constants

class MyClass:
    def __init__(self):
        self.attr = 1

    def dummy_return(self):
        return self.attr

def dummy_return():
    return 2

def bad_comparisons():
    """this is not ok"""
    instance = MyClass()
    for i in range(10):
        if 5 <= i:  # [misplaced-comparison-constant]
            pass
        if 1 == i:  # [misplaced-comparison-constant]
            pass
        if 3 < dummy_return():  # [misplaced-comparison-constant]
            pass
        if 4 != instance.dummy_return():  # [misplaced-comparison-constant]
            pass
        if 1 == instance.attr:  # [misplaced-comparison-constant]
            pass
        if "aaa" == instance.attr: # [misplaced-comparison-constant]
            pass

def good_comparison():
    """this is ok"""
    for i in range(10):
        if i == 5:
            pass

def double_comparison():
    """Check that we return early for non-binary comparison"""
    for i in range(10):
        if i == 1 == 2:
            pass
        if 2 <= i <= 8:
            print("Between 2 and 8 inclusive")

def const_comparison():
    """Check that we return early for comparison of two constants"""
    if 1 == 2:
        pass
