retriable_exceptions = (FileExistsError, FileNotFoundError)

try:
    pass
except (ValueError,):
    pass
except AttributeError:
    pass
except (ImportError, TypeError):
    pass
except (*retriable_exceptions,):
    pass
except(ValueError,):
    pass

list_exceptions = [FileExistsError, FileNotFoundError]

try:
    pass
except (*list_exceptions,):
    pass