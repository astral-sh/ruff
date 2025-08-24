breakpoint()

import pdb
import builtins
from builtins import breakpoint
from pdb import set_trace as st
from celery.contrib.rdb import set_trace
from celery.contrib import rdb
import celery.contrib.rdb
from debugpy import wait_for_client
import debugpy
from ptvsd import break_into_debugger
from ptvsd import enable_attach
from ptvsd import wait_for_attach
import ptvsd

breakpoint()
st()
set_trace()
debugpy.breakpoint()
wait_for_client()
debugpy.listen(1234)
enable_attach()
break_into_debugger()
wait_for_attach()


# also flag `breakpointhook` from `sys` but obviously not `sys` itself. see
# https://github.com/astral-sh/ruff/issues/16189
import sys # ok

def scope():
    from sys import breakpointhook # error

    breakpointhook() # error

def scope():
    from sys import __breakpointhook__ # error

    __breakpointhook__() # error

sys.breakpointhook() # error
sys.__breakpointhook__() # error
