import logging.config

t = logging.config.listen(9999)

def verify_func():
    pass

l = logging.config.listen(9999, verify=verify_func)
