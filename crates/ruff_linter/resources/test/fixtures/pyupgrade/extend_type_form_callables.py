from typing import Union

# This should NOT flag (position 0 is not checked)
my_custom_cast(Union[None, int], "hello")

# This SHOULD flag (position 1 is checked)
my_custom_cast("hello", Union[None, int])

# This SHOULD flag (keyword type_arg is checked)
my_custom_cast("hello", type_arg=Union[None, int])
