# flags: --line-ranges=11-17
# NOTE: If you need to modify this file, pay special attention to the --line-ranges=
# flag above as it's formatting specifically these lines.


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
