import logging
from logging import warn

logging.warn("this is not ok")
log.warn("this is also not ok")
warn("not ok")


def foo():
    from logging import warn

    def warn():
        pass

    warn("has been redefined, but we will still report it")
