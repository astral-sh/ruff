from os.path import join as łos  # Error
from os.path import join as los  # OK

import os.path.join as łos  # Error
import os.path.join as los  # OK

import os.path.łos  # Error (recommend an ASCII alias)
import os.path.los  # OK

from os.path import łos  # Error (recommend an ASCII alias)
from os.path import los  # OK

from os.path.łos import foo  # OK
from os.path.los import foo  # OK

from os.path import łos as foo  # OK
from os.path import los as foo  # OK
