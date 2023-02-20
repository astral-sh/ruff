# Errors
def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:
        logger.exception(f"Found an error: {ex}")


def main_function():
    try:
        process()
        handle()
        finish()
    except ValueError as bad:
        if True is False:
            for i in range(10):
                logger.exception(f"Found an error: {bad} {good}")
    except IndexError as bad:
        logger.exception(f"Foud an error: {bad} {bad}")
    except Exception as bad:
        logger.exception(f"Foud an error: {bad}")
        logger.exception(f"Foud an error: {bad}")


import logging

logger = logging.getLogger(__name__)


def func_fstr():
    try:
        ...
    except Exception as ex:
        logger.exception(f"log message {ex}")


def func_concat():
    try:
        ...
    except Exception as ex:
        logger.exception("log message: " + str(ex))


def func_comma():
    try:
        ...
    except Exception as ex:
        logger.exception("log message", ex)


# OK
def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:  # noqa: F841
        logger.exception(f"Found an error: {er}")
