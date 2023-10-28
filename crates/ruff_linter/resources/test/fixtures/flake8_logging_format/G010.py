import logging
from distutils import log

from logging_setup import logger

logging.warn("Hello World!")
log.warn("Hello world!")  # This shouldn't be considered as a logger candidate
logger.warn("Hello world!")

logging . warn("Hello World!")

from logging import warn, warning, exception
warn("foo")
warning("foo")
exception("foo")
