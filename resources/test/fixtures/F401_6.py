"""Test: explicit re-export."""

# OK
from .applications import FastAPI as FastAPI

# F401 `background.BackgroundTasks` imported but unused
from .background import BackgroundTasks

# F401 `datastructures.UploadFile` imported but unused
from .datastructures import UploadFile as FileUpload
