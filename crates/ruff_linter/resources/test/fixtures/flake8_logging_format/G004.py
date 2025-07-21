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

# Don't trigger for t-strings
info(t"{name}")
info(t"{__name__}")

count = 5
total = 9
directory_path = "/home/hamir/ruff/crates/ruff_linter/resources/test/"
logging.info(f"{count} out of {total} files in {directory_path} checked")
