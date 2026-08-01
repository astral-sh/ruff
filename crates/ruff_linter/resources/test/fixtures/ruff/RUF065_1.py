import logging

# Test cases for str() that should NOT be flagged (issue #21315)
# str() with no arguments - should not be flagged
logging.warning("%s", str())

# str() with multiple arguments - should not be flagged
logging.warning("%s", str(b"\xe2\x9a\xa0", "utf-8"))

# str() with starred arguments - should not be flagged
logging.warning("%s", str(*(b"\xf0\x9f\x9a\xa7", "utf-8")))

# str() with keyword unpacking - should not be flagged
logging.warning("%s", str(**{"object": b"\xf0\x9f\x9a\xa8", "encoding": "utf-8"}))

# str() with single keyword argument - should be flagged (equivalent to str("!"))
logging.warning("%s", str(object="!"))


# Complex conversion specifiers that make oct() and hex() necessary
# These should NOT be flagged because the behavior differs between %s and %#o/%#x
# https://github.com/astral-sh/ruff/issues/21458

# %06s with oct() - zero-pad flag with width (should NOT be flagged)
logging.warning("%06s", oct(123))

# % s with oct() - blank sign flag (should NOT be flagged)
logging.warning("% s", oct(123))

# %+s with oct() - sign char flag (should NOT be flagged)
logging.warning("%+s", oct(123))

# %.3s with hex() - precision (should NOT be flagged)
logging.warning("%.3s", hex(123))
