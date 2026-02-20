# This one is tricky because `array` is an exported
# symbol in a whole bunch of numpy internal modules.
#
# At time of writing (2025-10-07), the right completion
# doesn't actually show up at all in the suggestions
# returned. In fact, nothing from the top-level `numpy`
# module shows up.
arra<CURSOR: numpy.array>

import numpy as np
# In contrast to above, this *does* include the correct
# completion. So there is likely some kind of bug in our
# symbol discovery code for auto-import that isn't present
# when using ty to discover symbols (which is likely far
# too expensive to use across all dependencies).
np.arra<CURSOR: array>
