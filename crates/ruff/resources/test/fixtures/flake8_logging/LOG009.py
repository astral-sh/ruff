import logging
from logging import WARN, WARNING


logging.WARN  # LOG009


logging.WARNING  # OK


WARN  # LOG009


WARNING  # OK


logging.basicConfig(level=logging.WARN)  # LOG009


logging.basicConfig(level=logging.WARNING)  # OK


x = logging.WARN  # LOG009


y = WARN  # LOG009
