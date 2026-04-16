from .no_all import *          # RUF074 — no_all.py exists, has no __all__
from .has_all import *         # OK — has_all.py defines __all__
from .nonexistent import *     # OK — can't resolve, stay silent
from os.path import *          # OK — absolute import, not RUF074's job
from .no_all import join       # OK — not a wildcard import
