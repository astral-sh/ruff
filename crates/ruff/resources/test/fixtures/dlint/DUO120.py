import marshal


from marshal import load


# None marshal imports and funcs called marshal from other modules should not flag the rule:
from some_module import marshal

import os
