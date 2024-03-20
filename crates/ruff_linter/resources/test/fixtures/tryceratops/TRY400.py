"""
Violation:
Use '.exception' over '.error' inside except blocks
"""


def bad():
    import logging

    try:
        a = 1
    except Exception:
        logging.error("Context message here")

        if True:
            logging.error("Context message here")


def bad():
    import logging

    logger = logging.getLogger(__name__)

    try:
        a = 1
    except Exception:
        logger.error("Context message here")

        if True:
            logger.error("Context message here")


def bad():
    try:
        a = 1
    except Exception:
        log.error("Context message here")

        if True:
            log.error("Context message here")


def bad():
    try:
        a = 1
    except Exception:
        self.logger.error("Context message here")

        if True:
            self.logger.error("Context message here")


def good():
    import logging

    logger = logging.getLogger(__name__)

    try:
        a = 1
    except Exception:
        logger.exception("Context message here")


def good():
    try:
        a = 1
    except Exception:
        foo.exception("Context message here")


def fine():
    import logging

    logger = logging.getLogger(__name__)

    try:
        a = 1
    except Exception:
        logger.error("Context message here", exc_info=True)


def fine():
    import logging
    import sys

    logger = logging.getLogger(__name__)

    try:
        a = 1
    except Exception:
        logger.error("Context message here", exc_info=sys.exc_info())


def bad():
    from logging import error, exception

    try:
        a = 1
    except Exception:
        error("Context message here")

        if True:
            error("Context message here")


def good():
    from logging import error, exception

    try:
        a = 1
    except Exception:
        exception("Context message here")


def fine():
    from logging import error, exception

    try:
        a = 1
    except Exception:
        error("Context message here", exc_info=True)


def fine():
    from logging import error, exception
    import sys

    try:
        a = 1
    except Exception:
        error("Context message here", exc_info=sys.exc_info())


def nested():
    from logging import error, exception

    try:
        a = 1
    except Exception:
        try:
            b = 2
        except Exception:
            error("Context message here")
