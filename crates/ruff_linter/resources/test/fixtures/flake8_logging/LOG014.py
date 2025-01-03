import logging

# Logging ----------------------------------------------------------------------

logging.exception("foo")  # OK
logging.exception("foo", exc_info=False)  # OK
logging.exception("foo", exc_info=[])  # OK
logging.exception("foo", exc_info=True)  # LOG014
logging.exception("foo", exc_info=[1])  # LOG014

logging.error("foo", exc_info=False)  # OK
logging.error("foo", exc_info=True)  # LOG014

logging.info("foo", exc_info=False)  # OK
logging.info("foo", exc_info=True)  # LOG014

try:
    a = 1 * 2
except Exception:
    logging.exception("foo")  # OK
    logging.exception("foo", exc_info=False)  # OK
    logging.exception("foo", exc_info=[])  # OK
    logging.exception("foo", exc_info=True)  # OK
    logging.exception("foo", exc_info=[1])  # OK

    logging.error("foo", exc_info=False)  # OK
    logging.error("foo", exc_info=True)  # OK

    logging.info("foo", exc_info=False)  # OK
    logging.info("foo", exc_info=True)  # OK

# Logger -----------------------------------------------------------------------

logger = logging.getLogger(__name__)

logger.exception("foo")  # OK
logger.exception("foo", exc_info=False)  # OK
logger.exception("foo", exc_info=[])  # OK
logger.exception("foo", exc_info=True)  # LOG014
logger.exception("foo", exc_info=[1])  # LOG014

logger.error("foo", exc_info=False)  # OK
logger.error("foo", exc_info=True)  # LOG014

logger.info("foo", exc_info=False)  # OK
logger.info("foo", exc_info=True)  # LOG014

try:
    a = 1 * 2
except Exception:
    logger.exception("foo")  # OK
    logger.exception("foo", exc_info=False)  # OK
    logger.exception("foo", exc_info=[])  # OK
    logger.exception("foo", exc_info=True)  # OK
    logger.exception("foo", exc_info=[1])  # OK

    logger.error("foo", exc_info=False)  # OK
    logger.error("foo", exc_info=True)  # OK

    logger.info("foo", exc_info=False)  # OK
    logger.info("foo", exc_info=True)  # OK

# Direct Call ------------------------------------------------------------------

from logging import exception, error, info

exception("foo")  # OK
exception("foo", exc_info=False)  # OK
exception("foo", exc_info=[])  # OK
exception("foo", exc_info=True)  # LOG014
exception("foo", exc_info=[1])  # LOG014

error("foo", exc_info=False)  # OK
error("foo", exc_info=True)  # LOG014

info("foo", exc_info=False)  # OK
info("foo", exc_info=True)  # LOG014

try:
    a = 1 * 2
except Exception:
    exception("foo")  # OK
    exception("foo", exc_info=False)  # OK
    exception("foo", exc_info=[])  # OK
    exception("foo", exc_info=True)  # OK
    exception("foo", exc_info=[1])  # OK

    error("foo", exc_info=False)  # OK
    error("foo", exc_info=True)  # OK

    info("foo", exc_info=False)  # OK
    info("foo", exc_info=True)  # OK

# Fake Call --------------------------------------------------------------------

def wrapper():
    exception = lambda *args, **kwargs: None
    error = lambda *args, **kwargs: None
    info = lambda *args, **kwargs: None

    exception("foo")  # OK
    exception("foo", exc_info=False)  # OK
    exception("foo", exc_info=[])  # OK
    exception("foo", exc_info=True)  # OK
    exception("foo", exc_info=[1])  # OK

    error("foo", exc_info=False)  # OK
    error("foo", exc_info=True)  # OK

    info("foo", exc_info=False)  # OK
    info("foo", exc_info=True)  # OK

    try:
        a = 1 * 2
    except Exception:
        exception("foo")  # OK
        exception("foo", exc_info=False)  # OK
        exception("foo", exc_info=[])  # OK
        exception("foo", exc_info=True)  # OK
        exception("foo", exc_info=[1])  # OK

        error("foo", exc_info=False)  # OK
        error("foo", exc_info=True)  # OK

        info("foo", exc_info=False)  # OK
        info("foo", exc_info=True)  # OK

# Nested -----------------------------------------------------------------------

def apple_1():
    try:
        logging.exception("foo", exc_info=True)  # LOG014
    except Exception:
        pass


def apple_2():
    def banana():
        logging.exception("foo", exc_info=True)  # LOG014

    try:
        banana()
    except Exception:
        pass


def apple_3():
    def banana():
        logging.exception("foo", exc_info=True)  # LOG014

    try:
        a = 1 * 2
    except Exception:
        banana()
