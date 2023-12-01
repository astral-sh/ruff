"""
Should emit:
B030:
 - line 12, column 8
 - line 17, column 9
 - line 22, column 21
 - line 27, column 37
"""

try:
    pass
except 1:  # error
    pass

try:
    pass
except (1, ValueError):  # error
    pass

try:
    pass
except (ValueError, (RuntimeError, (KeyError, TypeError))):  # error
    pass

try:
    pass
except (ValueError, *(RuntimeError, (KeyError, TypeError))):  # error
    pass


try:
    pass
except (*a, *(RuntimeError, (KeyError, TypeError))):  # error
    pass

try:
    pass
except (ValueError, *(RuntimeError, TypeError)):  # ok
    pass

try:
    pass
except (ValueError, *[RuntimeError, *(TypeError,)]):  # ok
    pass


try:
    pass
except (*a, *b):  # ok
    pass


try:
    pass
except (*a, *(RuntimeError, TypeError)):  # ok
    pass


try:
    pass
except (*a, *(b, c)):  # ok
    pass


try:
    pass
except (*a, *(*b, *c)):  # ok
    pass


def what_to_catch():
    return ...


try:
    pass
except what_to_catch():  # ok
    pass
