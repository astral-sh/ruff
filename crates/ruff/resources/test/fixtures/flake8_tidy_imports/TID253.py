## Banned modules ##
import torch

from torch import *

from tensorflow import a, b, c

import torch as torch_wearing_a_trenchcoat

# this should count as module level
x = 1; import tensorflow

# banning a module also bans any submodules
import torch.foo.bar

from tensorflow.foo import bar

from torch.foo.bar import *

# unlike TID251, inline imports are *not* banned
def my_cool_function():
    import tensorflow.foo.bar

def another_cool_function():
    from torch.foo import bar

def import_alias():
    from torch.foo import bar

if TYPE_CHECKING:
    import torch
