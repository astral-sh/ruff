# OK: `except*` should not trigger SIM105 because `contextlib.suppress` doesn't
# support `BaseExceptionGroup` until Python 3.12, so the fix would be incorrect
# for Python 3.11. See https://github.com/astral-sh/ruff/issues/23798


def foo():
    pass


try:
    foo()
except* ValueError:
    pass


try:
    foo()
except* (ValueError, OSError):
    pass
