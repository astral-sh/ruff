# TODO(brent) this is accepted by 3.7 and 3.8 but not 3.13 (at least).
# looks like a separate change from the tuple unpacking checked here
def i(): return 1, (*rest)
