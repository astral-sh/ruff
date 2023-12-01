import logging

logging.info("Hello %s" % "World!")
logging.log(logging.INFO, "Hello %s" % "World!")

from logging import info, log

info("Hello %s" % "World!")
log(logging.INFO, "Hello %s" % "World!")
