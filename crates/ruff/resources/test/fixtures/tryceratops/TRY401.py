# Errors
def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:
        logger.exception(f"Found an error: {ex}")  # TRY401


def main_function():
    try:
        process()
        handle()
        finish()
    except ValueError as bad:
        if True is False:
            for i in range(10):
                logger.exception(f"Found an error: {bad} {good}")  # TRY401
    except IndexError as bad:
        logger.exception(f"Found an error: {bad} {bad}")  # TRY401
    except Exception as bad:
        logger.exception(f"Found an error: {bad}")  # TRY401
        logger.exception(f"Found an error: {bad}")  # TRY401

        if True:
            logger.exception(f"Found an error: {bad}")  # TRY401


import logging

logger = logging.getLogger(__name__)


def func_fstr():
    try:
        ...
    except Exception as ex:
        logger.exception(f"Logging an error: {ex}")  # TRY401


def func_concat():
    try:
        ...
    except Exception as ex:
        logger.exception("Logging an error: " + str(ex))  # TRY401


def func_comma():
    try:
        ...
    except Exception as ex:
        logger.exception("Logging an error:", ex)  # TRY401


# OK
def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:
        logger.exception(f"Found an error: {er}")
        logger.exception(f"Found an error: {ex.field}")
