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

# https://github.com/astral-sh/ruff/issues/26912
# A `*args` unpacking can supply any number of values, so arguments following it
# don't necessarily line up with the specifier in the same position.
args = ("a", "b")
logging.warning("%s%s%s%s %r", *"1234", str(5))
logging.warning("%s %s", *args, repr(5))
log(logging.INFO, "%s %s", *args, ascii(5))

# Arguments *before* the unpacking are still correctly aligned, so flag these.
logging.warning("%s %s", str(5), *args)
logging.warning("%s %s", repr(5), *args)

def str(s): return f"str = {s}"
# Don't flag this
logging.info("Hello %s", str("World!"))

logging.info("Debug info: %r", repr("test\nstring"))
logging.warning("Value: %r", repr(42))
logging.error("Error: %r", repr([1, 2, 3]))
logging.info("Debug info: %s", repr("test\nstring"))
logging.warning("Value: %s", repr(42))

# %s + ascii()
logging.info("ASCII: %s", ascii("Hello\nWorld"))
logging.warning("ASCII: %s", ascii("test"))

# %s + oct()
logging.info("Octal: %s", oct(42))
logging.warning("Octal: %s", oct(255))

# %s + hex()
logging.info("Hex: %s", hex(42))
logging.warning("Hex: %s", hex(255))


# Test with imported functions
from logging import info, log

info("ASCII: %s", ascii("Hello\nWorld"))
log(logging.INFO, "ASCII: %s", ascii("test"))

info("Octal: %s", oct(42))
log(logging.INFO, "Octal: %s", oct(255))

info("Hex: %s", hex(42))
log(logging.INFO, "Hex: %s", hex(255))

