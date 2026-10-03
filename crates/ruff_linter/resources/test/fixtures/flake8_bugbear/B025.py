"""
Should emit:
B025 - on lines 15, 22, 31, 40, 47, 56
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

try:
    a = 1
except* ValueError:
    a = 2
except* ValueError:
    a = 2

try:
    a = 1
except* pickle.PickleError:
    a = 2
except* ValueError:
    a = 2
except* pickle.PickleError:
    a = 2

try:
    a = 1
except* (ValueError, TypeError):
    a = 2
except* ValueError:
    a = 2
except* (OSError, TypeError):
    a = 2

# Hierarchy redundancy within a tuple does not change syntactic duplicate
# detection across handlers.
try:
    pass
except (OSError, TimeoutError):
    pass
except TimeoutError:
    pass

# B025 does not infer subclass relationships between separate handlers.
try:
    pass
except OSError:
    pass
except TimeoutError:
    pass

try:
    pass
except* (TimeoutError, OSError):
    pass
except* TimeoutError:
    pass
