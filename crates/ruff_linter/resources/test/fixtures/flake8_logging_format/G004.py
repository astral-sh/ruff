import logging

name = "world"
logging.info(f"Hello {name}")
logging.log(logging.INFO, f"Hello {name}")

_LOGGER = logging.getLogger()
_LOGGER.info(f"{__name__}")

logging.getLogger().info(f"{name}")

from logging import info

info(f"{name}")
info(f"{__name__}")
