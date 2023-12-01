import logging

logging.info(
    "Hello {world}!",
    extra=dict(
        world="{}".format("World"),
    ),
)
