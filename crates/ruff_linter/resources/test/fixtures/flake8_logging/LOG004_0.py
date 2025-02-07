from logging import exception as exc
import logging


logger = logging.getLogger(__name__)

# Tests adapted from:
# https://github.com/adamchainz/flake8-logging/blob/dbe88e7/tests/test_flake8_logging.py


### Errors

logging.exception("")
logger.exception("")
exc("")


def _():
    logging.exception("")
    logger.exception("")
    exc("")


try:
    ...
except ...:
    def _():
        logging.exception("")
        logger.exception("")
        exc("")


### No errors

try:
    ...
except ...:
    logging.exception("")
    logger.exception("")
    exc("")


def _():
    try:
        ...
    except ...:
        logging.exception("")
        logger.exception("")
        exc("")
