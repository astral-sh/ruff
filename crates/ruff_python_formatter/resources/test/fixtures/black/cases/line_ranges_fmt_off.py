# flags: --line-ranges=7-7 --line-ranges=17-23
# NOTE: If you need to modify this file, pay special attention to the --line-ranges=
# flag above as it's formatting specifically these lines.

# fmt: off
import   os
def   myfunc(  ):  # Intentionally unformatted.
    pass
# fmt: on


def   myfunc(  ):  # This will not be reformatted.
    print(  {"also won't be reformatted"}  )
# fmt: off
def   myfunc(  ):  # This will not be reformatted.
    print(  {"also won't be reformatted"}  )
def   myfunc(  ):  # This will not be reformatted.
    print(  {"also won't be reformatted"}  )
# fmt: on


def   myfunc(  ):  # This will be reformatted.
    print(  {"this will be reformatted"}  )
