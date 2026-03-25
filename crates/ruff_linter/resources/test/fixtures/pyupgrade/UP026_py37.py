# Skipped: AsyncMock was added to unittest.mock in 3.8
from mock import AsyncMock

# Skipped: ThreadingMock was added to unittest.mock in 3.13
from mock import ThreadingMock

# Skipped: any unavailable name suppresses the entire import
from mock import Mock, AsyncMock

# Error: all names available since 3.3
from mock import Mock

# Error: seal was added in 3.7, which matches the target
from mock import seal

# Error: all names available since 3.3
from mock import patch, MagicMock

# Error: star imports can't be checked
from mock import *

# Error: bare `import mock` always works (the module exists since 3.3)
import mock
