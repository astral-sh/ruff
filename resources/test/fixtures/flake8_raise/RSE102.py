try:
    y = 6 + "7"
except TypeError:
    raise ValueError()  # RSE102

try:
    x = 1 / 0
except ZeroDivisionError:
    raise

raise TypeError()  # RSE102

raise AssertionError

raise AttributeError("test message")
