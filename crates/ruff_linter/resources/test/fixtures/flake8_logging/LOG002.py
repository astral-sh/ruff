import logging
from logging import getLogger

# Ok
logging.getLogger(__name__)
logging.getLogger(name=__name__)
logging.getLogger("custom")
logging.getLogger(name="custom")

# LOG002
getLogger(__file__)
logging.getLogger(name=__file__)

logging.getLogger(__cached__)
getLogger(name=__cached__)


# Override `logging.getLogger`
class logging:
    def getLogger(self):
        pass


logging.getLogger(__file__)
