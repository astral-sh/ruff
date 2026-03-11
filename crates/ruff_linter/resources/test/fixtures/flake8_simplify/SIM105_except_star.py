# These should NOT trigger SIM105 for Python < 3.12 because
# `contextlib.suppress` only supports `BaseExceptionGroup` in Python 3.12+

# SIM105 should NOT be triggered for except* in Python < 3.12
def raise_group(x):
    raise ExceptionGroup("!", [ValueError(x)])


try:
    raise_group(1)
except* ValueError:
    pass


try:
    raise_group(1)
except* (ValueError, TypeError):
    pass


# For Python >= 3.12, these SHOULD trigger SIM105
# (but we test with py312 target version separately)
