from __future__ import annotations

import logging


def interpolate(logger, extra):
    logger.info("static value: %s", extra)  # OK
    logger.info(f"f-string value: {extra}")  # violation
    logger.warning("percent style %s" % extra)  # violation # noqa: UP031
    logging.error("format {}".format(extra))  # violation  # noqa: UP032
    logging.debug("plain literal")  # OK
