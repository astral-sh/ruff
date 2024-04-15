def func():
    import logging

    logging.WARN  # LOG009
    logging.WARNING  # OK


def func():
    from logging import WARN, WARNING

    WARN  # LOG009
    WARNING  # OK
