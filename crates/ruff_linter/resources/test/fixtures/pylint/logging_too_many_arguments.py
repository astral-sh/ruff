import logging

logging.warning("Hello %s", "World!", "again")  # [logging-too-many-args]

logging.warning("Hello %s", "World!", "again", something="else")

logging.warning("Hello %s", "World!")

# do not handle calls with *args
logging.error("Example log %s, %s", "foo", "bar", "baz", *args)

# do not handle calls with **kwargs
logging.error("Example log %s, %s", "foo", "bar", "baz", **kwargs)

# do not handle keyword arguments
logging.error("%(objects)d modifications: %(modifications)d errors: %(errors)d", {"objects": 1, "modifications": 1, "errors": 1})

logging.info(msg="Hello")

logging.info(msg="Hello", something="else")

import warning

warning.warning("Hello %s", "World!", "again")


from logging import info, error, warning

warning("Hello %s", "World!", "again")  # [logging-too-many-args]

warning("Hello %s", "World!", "again", something="else")

warning("Hello %s", "World!")

# do not handle calls with *args
error("Example log %s, %s", "foo", "bar", "baz", *args)

# do not handle calls with **kwargs
error("Example log %s, %s", "foo", "bar", "baz", **kwargs)

# do not handle keyword arguments
error("%(objects)d modifications: %(modifications)d errors: %(errors)d", {"objects": 1, "modifications": 1, "errors": 1})

info(msg="Hello")

info(msg="Hello", something="else")
