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


def still_actually_good():
    try:
        process()
    except MyException as e:
        try:
            pass
        except TypeError:
            raise e


def bad_that_needs_recursion():
    try:
        process()
    except MyException as e:
        logger.exception("process failed")
        if True:
            raise e


def bad_that_needs_recursion_2():
    try:
        process()
    except MyException as e:
        logger.exception("process failed")
        if True:
            def foo():
                raise e
