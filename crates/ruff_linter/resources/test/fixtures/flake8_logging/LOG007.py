import logging

logger = logging.getLogger(__name__)

logging.exception("foo")  # OK
logging.exception("foo", exc_info=False)  # LOG007
logging.exception("foo", exc_info=[])  # LOG007
logger.exception("foo")  # OK
logger.exception("foo", exc_info=False)  # LOG007
logger.exception("foo", exc_info=[])  # LOG007
logger.error("foo", exc_info=False)  # OK
logger.info("foo", exc_info=False)  # OK


from logging import exception

exception("foo", exc_info=False)  # LOG007
exception("foo", exc_info=True)  # OK


exception = lambda *args, **kwargs: None

exception("foo", exc_info=False)  # OK
exception("foo", exc_info=True)  # OK
