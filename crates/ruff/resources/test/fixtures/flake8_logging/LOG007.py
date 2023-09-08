import logging
from logging import exception


logging.exception("foo")  # OK


logging.exception("foo", exc_info=False)  # LOG007


logging.exception("foo", exc_info=[])  # LOG007


exception("foo", exc_info=False)  # LOG007


logging.exception("foo", exc_info=True)  # OK
