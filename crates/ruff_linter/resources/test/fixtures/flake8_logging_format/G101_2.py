import logging

logging.info(
    "Hello world!",
    extra=dict(
        name="foobar",
    ),
)

from logging import info

info(
    "Hello world!",
    extra=dict(
        name="foobar",
    ),
)
