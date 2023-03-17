import logging

logging.warning("Hello %s %s", "World!")  # [logging-too-few-args]

# do not handle calls with kwargs (like pylint)
logging.warning("Hello %s", "World!", "again", something="else")

logging.warning("Hello %s", "World!")

# do not handle calls without any args
logging.info("100% dynamic")

# do not handle calls with *args
logging.error("Example log %s, %s", "foo", "bar", "baz", *args)

# do not handle calls with **kwargs
logging.error("Example log %s, %s", "foo", "bar", "baz", **kwargs)

# do not handle keyword arguments
logging.error("%(objects)d modifications: %(modifications)d errors: %(errors)d")

import warning

warning.warning("Hello %s %s", "World!")
