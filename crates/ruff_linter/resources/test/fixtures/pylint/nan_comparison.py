import math
from math import nan as bad_val
import numpy as np
from numpy import nan as npy_nan


x = float('nan')
y = np.NaN

# PLW0117
if x == float('nan'):
    pass

# PLW0117
if x == float('NaN'):
    pass

# PLW0117
if x == float('NAN'):
    pass

# PLW0117
if x == float('Nan'):
    pass

# PLW0117
if x == math.nan:
    pass

# PLW0117
if x == bad_val:
    pass

# PLW0117
if y == np.NaN:
    pass

# PLW0117
if y == np.NAN:
    pass

# PLW0117
if y == np.nan:
    pass

# PLW0117
if y == npy_nan:
    pass

# Ok
if math.isnan(x):
    pass

# Ok
if np.isnan(y):
    pass

# Ok
if x == 0:
    pass

# Ok
if x == float('32'):
    pass

# Ok
if x == float(42):
    pass

# Ok
if y == np.inf:
    pass

# Ok
if x == 'nan':
    pass
