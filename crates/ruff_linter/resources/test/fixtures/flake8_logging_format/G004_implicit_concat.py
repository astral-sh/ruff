import logging

variablename = "value"

log = logging.getLogger(__name__)
log.info(f"a" f"b {variablename}")
log.info("a " f"b {variablename}")
log.info("prefix " f"middle {variablename}" f" suffix")
