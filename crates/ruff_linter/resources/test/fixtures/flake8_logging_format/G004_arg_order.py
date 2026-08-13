"""Test f-string argument order."""

import logging

logger = logging.getLogger(__name__)

X = 1
Y = 2
logger.error(f"{X} -> %s", Y)
logger.error(f"{Y} -> %s", X)
