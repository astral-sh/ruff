import math
import numpy as np


x = float('nan')
y = np.NaN

# PLW0117
is_nan = x == float('nan')

# PLW0117
is_nan = y == np.NaN

# Ok
math.isnan(x)
np.isnan(y)
