# Too many args (6/4) for max_args=4
# OK for dummy_variable_rgx ~ "skip_.*"
def f(x, y, z, skip_t, skip_u, skip_v):
    pass


# Too many args (6/4) for max_args=4
# Too many args (6/5) for dummy_variable_rgx ~ "skip_.*"
def f(x, y, z, t, u, v):
    pass
