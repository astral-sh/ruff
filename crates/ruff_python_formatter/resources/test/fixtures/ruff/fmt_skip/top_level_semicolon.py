    # 0
'''Module docstring
multiple lines'''      # 1 # fmt: skip # 2

import a;import b; from c import (
        x,y,z
    ); import f # fmt: skip

x=1;x=2;x=3;x=4 # fmt: skip

   # 1
x=[     # 2
    '1',       # 3
    '2',
    '3'
];x=1;x=1 # 4 # fmt: skip # 5
   # 6

def foo(): x=[
    '1',
    '2',
];x=1 # fmt: skip
