import logging

logging.exception("foo")  # LOG004
logging.info("shouldnt_be_found")  # OK
try:
    logging.exception("bar")  # LOG004
    logging.info("baz")  # OK
    _ = 1 / 0
except ZeroDivisionError:
    logging.exception("bar")  # OK
    logging.info("baz")  # OK

    def handle():
        logging.exception("qux")  # LOG004
