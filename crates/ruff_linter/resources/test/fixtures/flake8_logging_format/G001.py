import logging
import logging as foo

logging.info("Hello {}".format("World!"))
logging.log(logging.INFO, "Hello {}".format("World!"))
foo.info("Hello {}".format("World!"))
logging.log(logging.INFO, msg="Hello {}".format("World!"))
logging.log(level=logging.INFO, msg="Hello {}".format("World!"))
logging.log(msg="Hello {}".format("World!"), level=logging.INFO)

# Flask support
import flask
from flask import current_app
from flask import current_app as app

flask.current_app.logger.info("Hello {}".format("World!"))
current_app.logger.info("Hello {}".format("World!"))
app.logger.log(logging.INFO, "Hello {}".format("World!"))

from logging import info, log

info("Hello {}".format("World!"))
log(logging.INFO, "Hello {}".format("World!"))
info("Hello {}".format("World!"))
log(logging.INFO, msg="Hello {}".format("World!"))
log(level=logging.INFO, msg="Hello {}".format("World!"))
log(msg="Hello {}".format("World!"), level=logging.INFO)
