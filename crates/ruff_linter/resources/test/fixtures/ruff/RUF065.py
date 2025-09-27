import logging

# %s + str()
logging.info("Hello %s", str("World!"))
logging.log(logging.INFO, "Hello %s", str("World!"))

# %s + repr()
logging.info("Hello %s", repr("World!"))
logging.log(logging.INFO, "Hello %s", repr("World!"))

# %r + str()
logging.info("Hello %r", str("World!"))
logging.log(logging.INFO, "Hello %r", str("World!"))

# %r + repr()
logging.info("Hello %r", repr("World!"))
logging.log(logging.INFO, "Hello %r", repr("World!"))

from logging import info, log

# %s + str()
info("Hello %s", str("World!"))
log(logging.INFO, "Hello %s", str("World!"))

# %s + repr()
info("Hello %s", repr("World!"))
log(logging.INFO, "Hello %s", repr("World!"))

# %r + str()
info("Hello %r", str("World!"))
log(logging.INFO, "Hello %r", str("World!"))

# %r + repr()
info("Hello %r", repr("World!"))
log(logging.INFO, "Hello %r", repr("World!"))

def str(s): return f"str = {s}"
# Don't flag this
logging.info("Hello %s", str("World!"))

logging.info("Debug info: %r", repr("test\nstring"))
logging.warning("Value: %r", repr(42))
logging.error("Error: %r", repr([1, 2, 3]))
logging.info("Debug info: %s", repr("test\nstring"))
logging.warning("Value: %s", repr(42))
