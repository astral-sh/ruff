import logging
import sys

# G202
try:
    pass
except:
    logging.exception("Hello World", exc_info=True)

try:
    pass
except:
    logging.exception("Hello World", exc_info=sys.exc_info())

# OK
try:
    pass
except:
    logging.exception("Hello World", exc_info=False)

logging.exception("Hello World", exc_info=True)
