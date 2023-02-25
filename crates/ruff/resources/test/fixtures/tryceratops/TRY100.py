"""
Violation:

Constantly checking for non None values may indicate your parent function
should be raising an exception instead of returning
"""


def another_func():
    return None  # should raise instead


def func() -> int:
    """This function uses 'a' and 'b' to ask for permission
    to continue. Its final goal is to arrive to the last return line
    successfully which makes hard for devs to understand "when"
    cases.
    """
    a = another_func()
    if not a:
        return

    b = another_func()
    if b:
        return -1

    result = 10
    return result


def good() -> bool:
    """Outlines a good case, the function builds vars
    to return the final result, it doesn't ask for permission
    to keep going.
    """
    a = 1 == 1
    if a:
        return True

    b = a is True
    if b:
        return True

    return False
