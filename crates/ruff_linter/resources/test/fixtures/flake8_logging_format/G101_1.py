import logging

logging.info(
    "Hello world!",
    extra={
        "name": "foobar",
    },
)

from logging import info

info(
    "Hello world!",
    extra={
        "name": "foobar",
    },
)
