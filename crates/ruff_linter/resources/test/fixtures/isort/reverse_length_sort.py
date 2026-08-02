from datetime import datetime
from typing import TYPE_CHECKING
from uuid import UUID

from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.orm import mapped_column, relationship, Mapped
from sqlalchemy import ForeignKey, String

from ..base import Base
from ..mixins import CreatedMixin
