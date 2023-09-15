import logging

logging.exception("foo")  # OK
logging.exception("foo", exc_info=False)  # LOG007
logging.exception("foo", exc_info=[])  # LOG007

from logging import exception

exception("foo", exc_info=False)  # LOG007
exception("foo", exc_info=True)  # OK
