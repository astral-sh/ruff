import logging

logging.info(
    "Hello {world}!",
    extra=dict(
        world="World",
    ),
)
