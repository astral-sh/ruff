import logging

logging.warning("Hello %s", "World!", "again")  # [logging-too-many-args]

logging.warning("Hello %s", "World!", "again", something="else")

logging.warning("Hello %s", "World!")

# do not handle calls with *args
logging.error("Example log %s, %s", "foo", "bar", "baz", *args)

# do not handle calls with **kwargs
logging.error("Example log %s, %s", "foo", "bar", "baz", **kwargs)

import warning

warning.warning("Hello %s", "World!", "again")
