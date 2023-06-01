from typing import TYPE_CHECKING
from django.db.models import ForeignKey

if TYPE_CHECKING:
    from pathlib import Path


class Foo:
    var = ForeignKey["Path"]()
