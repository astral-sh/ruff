import sys
import tempfile
from pprint import pprint

pprint("Hello, world!")  # T203
pprint("Hello, world!", stream=None)  # T203
pprint("Hello, world!", stream=sys.stdout)  # T203
pprint("Hello, world!", stream=sys.stderr)  # T203

with tempfile.NamedTemporaryFile() as fp:
    pprint("Hello, world!", stream=fp)  # OK

import pprint

pprint.pprint("Hello, world!")  # T203
pprint.pprint("Hello, world!", stream=None)  # T203
pprint.pprint("Hello, world!", stream=sys.stdout)  # T203
pprint.pprint("Hello, world!", stream=sys.stderr)  # T203

with tempfile.NamedTemporaryFile() as fp:
    pprint.pprint("Hello, world!", stream=fp)  # OK

pprint.pformat("Hello, world!")
