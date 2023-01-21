"""
Violation:

Raising an exception using its assigned name is verbose and unrequired
"""
import logging

logger = logging.getLogger(__name__)


class MyException(Exception):
    pass


def bad():
    try:
        process()
    except MyException as e:
        logger.exception("process failed")
        raise e


def good():
    try:
        process()
    except MyException:
        logger.exception("process failed")
        raise

def still_good():
    try:
        process()
    except MyException as e:
        print(e)
        raise
