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



x = 99
fmt = "08d"
logger.info(f"{x:{'08d'}}")
logger.info(f"{x:>10} {x:{fmt}}")

logging.info(f"")
logging.info(f"This message doesn't have any variables.")

obj = {"key": "value"}
logging.info(f"Object: {obj!r}")

items_count = 3
logging.warning(f"Items: {items_count:d}")

data = {"status": "active"}
logging.info(f"Processing {len(data)} items")
logging.info(f"Status: {data.get('status', 'unknown').upper()}")


result = 123
logging.info(f"Calculated result: {result + 100}")

temperature = 123
logging.info(f"Temperature: {temperature:.1f}°C")

class FilePath:
    def __init__(self, name: str):
        self.name = name

logging.info(f"No changes made to {file_path.name}.")

user = "tron"
balance = 123.45
logging.error(f"Error {404}: User {user} has insufficient balance ${balance:.2f}")

import logging

x = 1
logging.error(f"{x} -> %s", x)
