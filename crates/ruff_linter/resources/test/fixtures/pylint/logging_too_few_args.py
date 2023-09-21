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

logging.info(msg="Hello %s")

logging.info(msg="Hello %s %s")

import warning

warning.warning("Hello %s %s", "World!")


from logging import error, info, warning

warning("Hello %s %s", "World!")  # [logging-too-few-args]

# do not handle calls with kwargs (like pylint)
warning("Hello %s", "World!", "again", something="else")

warning("Hello %s", "World!")

# do not handle calls without any args
info("100% dynamic")

# do not handle calls with *args
error("Example log %s, %s", "foo", "bar", "baz", *args)

# do not handle calls with **kwargs
error("Example log %s, %s", "foo", "bar", "baz", **kwargs)

# do not handle keyword arguments
error("%(objects)d modifications: %(modifications)d errors: %(errors)d")

info(msg="Hello %s")

info(msg="Hello %s %s")
