"""Case: except* and except with py311 and higher"""

# SIM105
try:
    pass
except* ValueError:
    pass

# SIM105
try:
    pass
except ValueError:
    pass
