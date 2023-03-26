import logging
import logging as foo

logging.info("Hello {}".format("World!"))
logging.log(logging.INFO, "Hello {}".format("World!"))
foo.info("Hello {}".format("World!"))
logging.log(logging.INFO, msg="Hello {}".format("World!"))
logging.log(level=logging.INFO, msg="Hello {}".format("World!"))
logging.log(msg="Hello {}".format("World!"), level=logging.INFO)
