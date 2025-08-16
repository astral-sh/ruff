import logging

logging.info("Hello %s", str("World!"))
logging.info("Hello %s", repr("World!"))
logging.log(logging.INFO, "Hello %s", str("World!"))
logging.log(logging.INFO, "Hello %s", repr("World!"))

from logging import info, log

info("Hello %s", str("World!"))
info("Hello %s", repr("World!"))
log(logging.INFO, "Hello %s", str("World!"))
log(logging.INFO, "Hello %s", repr("World!"))
