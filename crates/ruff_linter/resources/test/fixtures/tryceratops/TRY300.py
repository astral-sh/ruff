"""
Violation:

Returning a final value inside a try block may indicate you could use an else block
instead to outline the success scenario
"""
import logging

logger = logging.getLogger(__name__)


class MyException(Exception):
    pass


def bad():
    try:
        a = 1
        b = process()
        return b
    except MyException:
        logger.exception("process failed")


def good():
    try:
        a = 1
        b = process()
    except MyException:
        logger.exception("process failed")
    else:
        return b


def noreturn():
    try:
        a = 1
        b = process()
    except MyException:
        logger.exception("process failed")


def good_return_with_side_effects():
    try:
        pass
        return process()
    except MyException:
        logger.exception("process failed")

def good_noexcept():
    try:
        pass
        return process()
    finally:
        logger.exception("process failed")
