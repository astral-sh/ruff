"""Check for else branches on loops with break and return only."""


def test_return_for():
    """else + return is not acceptable."""
    for i in range(10):
        if i % 2:
            return i
    else:  # [useless-else-on-loop]
        print("math is broken")
    return None


def test_return_while():
    """else + return is not acceptable."""
    while True:
        return 1
    else:  # [useless-else-on-loop]
        print("math is broken")
    return None


while True:

    def short_fun():
        """A function with a loop."""
        for _ in range(10):
            break

else:  # [useless-else-on-loop]
    print("or else!")


while True:
    while False:
        break
else:  # [useless-else-on-loop]
    print("or else!")

for j in range(10):
    pass
else:  # [useless-else-on-loop]
    print("fat chance")
    for j in range(10):
        break


def test_return_for2():
    """no false positive for break in else

    https://bitbucket.org/logilab/pylint/issue/117/useless-else-on-loop-false-positives
    """
    for i in range(10):
        for _ in range(i):
            if i % 2:
                break
        else:
            break
    else:
        print("great math")


def test_break_in_orelse_deep():
    """no false positive for break in else deeply nested"""
    for _ in range(10):
        if 1 < 2:  # pylint: disable=comparison-of-constants
            for _ in range(3):
                if 3 < 2:  # pylint: disable=comparison-of-constants
                    break
            else:
                break
    else:
        return True
    return False


def test_break_in_orelse_deep2():
    """should raise a useless-else-on-loop message, as the break statement is only
    for the inner for loop
    """
    for _ in range(10):
        if 1 < 2:  # pylint: disable=comparison-of-constants
            for _ in range(3):
                if 3 < 2:  # pylint: disable=comparison-of-constants
                    break
            else:
                print("all right")
    else:  # [useless-else-on-loop]
        return True
    return False


def test_break_in_orelse_deep3():
    """no false positive for break deeply nested in else"""
    for _ in range(10):
        for _ in range(3):
            pass
        else:
            if 1 < 2:  # pylint: disable=comparison-of-constants
                break
    else:
        return True
    return False


def test_break_in_if_orelse():
    """should raise a useless-else-on-loop message due to break in else"""
    for _ in range(10):
        if 1 < 2:  # pylint: disable=comparison-of-constants
            pass
        else:
            break
    else:
        return True
    return False
