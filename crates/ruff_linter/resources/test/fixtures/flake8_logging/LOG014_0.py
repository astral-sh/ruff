import logging


logger = logging.getLogger(__name__)

# Tests adapted from:
# https://github.com/adamchainz/flake8-logging/blob/dbe88e7/tests/test_flake8_logging.py


### Errors

logging.info("", exc_info=True)
logger.info("", exc_info=True)


logging.info("", exc_info=1)
logger.info("", exc_info=1)


def _():
    logging.info("", exc_info=True)
    logger.info("", exc_info=True)


try:
    ...
except ...:
    def _():
        logging.info("", exc_info=True)
        logger.info("", exc_info=True)


### No errors

logging.info("", exc_info=a)
logger.info("", exc_info=a)

logging.info("", exc_info=False)
logger.info("", exc_info=False)


try:
    ...
except ...:
    logging.info("", exc_info=True)
    logger.info("", exc_info=True)


def _():
    try:
        ...
    except ...:
        logging.info("", exc_info=True)
        logger.info("", exc_info=True)


some_variable_that_ends_with_logger.not_a_recognized_method("", exc_info=True)
