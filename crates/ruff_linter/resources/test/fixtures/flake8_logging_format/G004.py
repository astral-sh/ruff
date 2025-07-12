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

# Test Multiline f-string with multiple variables
the_total_number_of_files = 100
the_number_of_files_changed = 25
directory_path = "/home/hamir/project"

logging.info(
    f'Cleaned {the_number_of_files_changed} out of {the_total_number_of_files} files in {directory_path}.'
)

# Test Attribute access in f-string
class FilePath:
    def __init__(self, name):
        self.name = name

file_path = FilePath("config.py")
logging.info(f'No changes made to {file_path.name}.')

# Test Mixed format specifiers in f-string
user_id = 12345
username = "alice"
balance = 1234.56
logging.error(f'Error {404}: User {username} has insufficient balance ${balance:.2f}')

# Test Complex expressions in f-string
data = {"status": "success", "count": 100}
logging.info(f'Processing {len(data)} items')
logging.debug(f'Status: {data.get("status", "unknown").upper()}')

# Test Various format specifiers
temperature = 23.7
items_count = 42
logging.debug(f'Temperature: {temperature:.1f}°C')
logging.warning(f'Items: {items_count:d}')

# Test Repr formatting
obj = {"key": "value"}
logging.info(f'Object: {obj!r}')

# Test Empty and text-only f-strings
logging.debug(f'')
logging.info(f'Static message without variables')

# Test Nested expressions
result = user_id * 2
logging.info(f'Calculated result: {result + 100}')
