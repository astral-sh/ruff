try:
    y = 6 + "7"
except TypeError:
    raise ValueError()  # R102

try:
    x = 1 / 0
except ZeroDivisionError:
    raise

raise TypeError()  # R102

raise AssertionError

raise AttributeError("test message")
