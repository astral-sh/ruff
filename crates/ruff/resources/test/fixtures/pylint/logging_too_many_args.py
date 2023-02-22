import logging

logging.warning("Hello %s", "World!", "again")  # [logging-too-many-args]

# do not handle calls with kwargs (like pylint)
logging.warning("Hello %s", "World!", "again", something="else")

logging.warning("Hello %s", "World!")

import warning

warning.warning("Hello %s", "World!", "again")
