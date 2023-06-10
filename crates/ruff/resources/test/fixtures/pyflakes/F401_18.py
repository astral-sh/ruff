"""Testing that multiple submodule imports are handled correctly."""

# The logic goes through each of the shadowed bindings upwards to get all the unused
# imports. It should only detect imports, not any other kind of binding.
multiprocessing = None

import multiprocessing.connection
import multiprocessing.pool
import multiprocessing.queues
