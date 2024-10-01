import math
from math import nan as bad_val
import numpy as np
from numpy import nan as npy_nan


x = float("nan")
y = np.NaN

# PLW0117
if x == float("nan"):
    pass

# PLW0117
if x == float("NaN"):
    pass

# PLW0117
if x == float("NAN"):
    pass

# PLW0117
if x == float("Nan"):
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

import builtins

# PLW0117
if x == builtins.float("nan"):
    pass

# OK
if math.isnan(x):
    pass

# OK
if np.isnan(y):
    pass

# OK
if x == 0:
    pass

# OK
if x == float("32"):
    pass

# OK
if x == float(42):
    pass

# OK
if y == np.inf:
    pass

# OK
if x == "nan":
    pass
