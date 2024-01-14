import re

# BAD:
if re.match("^hello", "hello world", re.I):
    pass


# GOOD:
if re.match("^hello", "hello world", re.IGNORECASE):
    pass
