# All errors
import logging

logging.debug("Lorem")
logging.info("ipsum")
logging.warn("dolor")
logging.warning("sit")
logging.error("amet")
logging.critical("consectetur")
logging.log("adipiscing")
logging.exception("elit.")


# All errors
from logging import (
    debug,
    info,
    warn,
    warning,
    error,
    critical,
    log,
    exception
)

debug("Lorem")
info("ipsum")
warn("dolor")
warning("sit")
error("amet")
critical("consectetur")
log("adipiscing")
exception("elit.")


# No errors
logger = logging.getLogger("")

logger.debug("Lorem")
logger.info("ipsum")
logger.warn("dolor")
logger.warning("sit")
logger.error("amet")
logger.critical("consectetur")
logger.log("adipiscing")
logger.exception("elit.")


# No errors
logging = logging.getLogger("")

logging.debug("Lorem")
logging.info("ipsum")
logging.warn("dolor")
logging.warning("sit")
logging.error("amet")
logging.critical("consectetur")
logging.log("adipiscing")
logging.exception("elit.")
