"""
Violation:
Use '.exception' over '.error' inside except blocks
"""

import logging

logger = logging.getLogger(__name__)

def bad():
    try:
        a = 1
    except Exception:
        logger.error("Context message here")

def good():
    try:
        a = 1
    except Exception:
        logger.exception("Context message here")
