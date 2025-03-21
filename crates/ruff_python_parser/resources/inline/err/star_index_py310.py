# parse_options: {"target-version": "3.10"}
lst[*index]  # simple index
class Array(Generic[DType, *Shape]): ...  # motivating example from the PEP
lst[a, *b, c]  # different positions
lst[a, b, *c]  # different positions
lst[*a, *b]  # multiple unpacks
array[3:5, *idxs]  # mixed with slices
