# Input types accepted by Pydantic validation in lax mode. These aliases model Python inputs from
# Pydantic's conversion table: https://docs.pydantic.dev/latest/concepts/conversion_table/

from datetime import date, datetime, time, timedelta
from decimal import Decimal
from ipaddress import (
    IPv4Address,
    IPv4Interface,
    IPv4Network,
    IPv6Address,
    IPv6Interface,
    IPv6Network,
)
from pathlib import Path
from re import Pattern
from uuid import UUID

type LaxBool = bool | float | int | str | Decimal
type LaxBytes = bytearray | bytes | str
type LaxByteSize = float | int | str | Decimal
type LaxDate = bytes | date | datetime | float | int | str | Decimal
type LaxDatetime = bytes | date | datetime | float | int | str | Decimal
type LaxDecimal = float | int | str | Decimal
type LaxFloat = bool | bytes | float | int | str | Decimal
type LaxInt = bool | bytes | float | int | str | Decimal
type LaxIPv4Address = bytes | int | str | IPv4Address | IPv4Interface
type LaxIPv4Interface = (
    bytes | int | str | tuple[object, object] | IPv4Address | IPv4Interface
)
type LaxIPv4Network = bytes | int | str | IPv4Address | IPv4Interface | IPv4Network
type LaxIPv6Address = bytes | int | str | IPv6Address | IPv6Interface
type LaxIPv6Interface = (
    bytes | int | str | tuple[object, object] | IPv6Address | IPv6Interface
)
type LaxIPv6Network = bytes | int | str | IPv6Address | IPv6Interface | IPv6Network
type LaxPath = str | Path
type LaxStrPattern = str | Pattern[str]
type LaxBytesPattern = bytes | Pattern[bytes]
type LaxStr = bytearray | bytes | str
type LaxTime = bytes | float | int | str | time | Decimal
type LaxTimedelta = bytes | float | int | str | timedelta | Decimal
type LaxUUID = str | UUID
