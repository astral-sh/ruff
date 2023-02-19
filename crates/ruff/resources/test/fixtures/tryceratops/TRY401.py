# These SHOULD change


def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:
        logger.exception(f"Found an error: {ex}")


def main_function():
    try:
        process()
        handle()
        finish()
    except ValueError as bad:
        if True is False:
            for i in range(10):
                logger.exception(f"Found an error: {bad} {good}")
    except IndexError as bad:
        logger.exception(f"Foud an error: {bad} {bad}")
    except Exception as bad:
        logger.exception(f"Foud an error: {bad}")
        logger.exception(f"Foud an error: {bad}")


# These should NOT change


def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:  # noqa: F841
        logger.exception(f"Found an error: {er}")
