import pickle

pickle.loads()


# https://github.com/astral-sh/ruff/issues/15522
map(pickle.load, [])
foo = pickle.load
