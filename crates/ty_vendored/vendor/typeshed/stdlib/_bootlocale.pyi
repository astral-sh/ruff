"""A minimal subset of the locale module used at interpreter startup
(imported by the _io module), in order to reduce startup time.

Don't import directly from third-party code; use the `locale` module instead!
"""

def getpreferredencoding(do_setlocale: bool = True) -> str: ...
