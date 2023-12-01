import logging

logging.info("Hello" + " " + "World!")
logging.log(logging.INFO, "Hello" + " " + "World!")

from logging import info, log

info("Hello" + " " + "World!")
log(logging.INFO, "Hello" + " " + "World!")
