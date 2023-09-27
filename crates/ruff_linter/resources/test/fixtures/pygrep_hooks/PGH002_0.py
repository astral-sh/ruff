import logging
import warnings
from warnings import warn

warnings.warn("this is ok")
warn("by itself is also ok")
logging.warning("this is fine")

logger = logging.getLogger(__name__)
logger.warning("this is fine")
