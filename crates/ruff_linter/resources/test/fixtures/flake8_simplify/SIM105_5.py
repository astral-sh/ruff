# Regression test for: https://github.com/astral-sh/ruff/issues/23798
# SIM105 should be suppressed for except* in Python 3.11

# OK - except* should not trigger SIM105
try:
    foo()
except* ValueError:
    pass

# OK - except* should not trigger SIM105
try:
    foo()
except* (ValueError, OSError):
    pass

# OK - except* with ExceptionGroup should not trigger SIM105
try:
    foo()
except* ExceptionGroup:
    pass
