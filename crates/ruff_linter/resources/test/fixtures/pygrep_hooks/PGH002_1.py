import logging
from logging import warn

logging.warn("this is not ok")
warn("not ok")

logger = logging.getLogger(__name__)
logger.warn("this is not ok")
