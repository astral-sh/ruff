"""
Should emit:
B025 - on lines 15, 22, 31
"""

import pickle

try:
    a = 1
except ValueError:
    a = 2
finally:
    a = 3

try:
    a = 1
except ValueError:
    a = 2
except ValueError:
    a = 2

try:
    a = 1
except pickle.PickleError:
    a = 2
except ValueError:
    a = 2
except pickle.PickleError:
    a = 2

try:
    a = 1
except (ValueError, TypeError):
    a = 2
except ValueError:
    a = 2
except (OSError, TypeError):
    a = 2
