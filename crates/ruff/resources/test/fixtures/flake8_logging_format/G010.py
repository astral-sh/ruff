import logging
from distutils import log

logging.warn("Hello World!")
log.warn("Hello world!")  # This shouldn't be considered as a logger candidate
