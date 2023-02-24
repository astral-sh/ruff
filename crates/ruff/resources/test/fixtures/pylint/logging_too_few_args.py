import logging

logging.warning("Hello %s %s", "World!")  # [logging-too-few-args]

# do not handle calls with kwargs (like pylint)
logging.warning("Hello %s", "World!", "again", something="else")

logging.warning("Hello %s", "World!")

# do not handle calls without any args
logging.info("100% dynamic")

import warning

warning.warning("Hello %s %s", "World!")
