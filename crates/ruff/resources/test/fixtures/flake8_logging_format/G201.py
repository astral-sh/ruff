import logging
import sys

# G201
try:
    pass
except:
    logging.error("Hello World", exc_info=True)

try:
    pass
except:
    logging.error("Hello World", exc_info=sys.exc_info())

# OK
try:
    pass
except:
    logging.error("Hello World", exc_info=False)

logging.error("Hello World", exc_info=sys.exc_info())
