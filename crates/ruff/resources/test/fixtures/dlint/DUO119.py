import shelve


from shelve import open


# None shelve imports and funcs called shelve from other modules should not flag the rule:
from some_module import shelve

import os
