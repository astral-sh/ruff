try:
    1 / 0
except ZeroDivisionError or ValueError as e:  # [binary-op-exception]
    pass

try:
    raise ValueError
except ZeroDivisionError and ValueError as e:  # [binary-op-exception]
    print(e)

try:
    pass
except (ValueError, Exception, IOError):
    pass
