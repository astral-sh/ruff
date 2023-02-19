def main_function():
    try:
        process()
        handle()
        finish()
    except Exception as ex:
        logger.exception(f"Found an error: {ex}")
