from __future__ import annotations as _annotations

import abc
import dataclasses as _dataclasses
import re
from datetime import date, datetime
from decimal import Decimal
from enum import Enum
from pathlib import Path
from typing import (
    TYPE_CHECKING,
    Any,
    ClassVar,
    FrozenSet,
    Generic,
    Hashable,
    List,
    Set,
    Tuple,
    Type,
    TypeVar,
    Union,
    cast,
)
from uuid import UUID

import annotated_types
from pydantic_core import PydanticCustomError, PydanticKnownError, core_schema
from typing_extensions import Annotated, Literal

from ._internal import _fields, _validators

__all__ = [
    'Strict',
    'StrictStr',
    'conbytes',
    'conlist',
    'conset',
    'confrozenset',
    'constr',
    'ImportString',
    'conint',
    'PositiveInt',
    'NegativeInt',
    'NonNegativeInt',
    'NonPositiveInt',
    'confloat',
    'PositiveFloat',
    'NegativeFloat',
    'NonNegativeFloat',
    'NonPositiveFloat',
    'FiniteFloat',
    'condecimal',
    'UUID1',
    'UUID3',
    'UUID4',
    'UUID5',
    'FilePath',
    'DirectoryPath',
    'Json',
    'SecretField',
    'SecretStr',
    'SecretBytes',
    'StrictBool',
    'StrictBytes',
    'StrictInt',
    'StrictFloat',
    'PaymentCardNumber',
    'ByteSize',
    'PastDate',
    'FutureDate',
    'condate',
    'AwareDatetime',
    'NaiveDatetime',
]

from ._internal._core_metadata import build_metadata_dict
from ._internal._utils import update_not_none
from .json_schema import JsonSchemaMetadata


@_dataclasses.dataclass
class Strict(_fields.PydanticMetadata):
    strict: bool = True


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ BOOLEAN TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

StrictBool = Annotated[bool, Strict()]

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ INTEGER TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


def conint(
    *,
    strict: bool | None = None,
    gt: int | None = None,
    ge: int | None = None,
    lt: int | None = None,
    le: int | None = None,
    multiple_of: int | None = None,
) -> type[int]:
    return Annotated[  # type: ignore[return-value]
        int,
        Strict(strict) if strict is not None else None,
        annotated_types.Interval(gt=gt, ge=ge, lt=lt, le=le),
        annotated_types.MultipleOf(multiple_of) if multiple_of is not None else None,
    ]


PositiveInt = Annotated[int, annotated_types.Gt(0)]
NegativeInt = Annotated[int, annotated_types.Lt(0)]
NonPositiveInt = Annotated[int, annotated_types.Le(0)]
NonNegativeInt = Annotated[int, annotated_types.Ge(0)]
StrictInt = Annotated[int, Strict()]

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ FLOAT TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


@_dataclasses.dataclass
class AllowInfNan(_fields.PydanticMetadata):
    allow_inf_nan: bool = True


def confloat(
    *,
    strict: bool | None = None,
    gt: float | None = None,
    ge: float | None = None,
    lt: float | None = None,
    le: float | None = None,
    multiple_of: float | None = None,
    allow_inf_nan: bool | None = None,
) -> type[float]:
    return Annotated[  # type: ignore[return-value]
        float,
        Strict(strict) if strict is not None else None,
        annotated_types.Interval(gt=gt, ge=ge, lt=lt, le=le),
        annotated_types.MultipleOf(multiple_of) if multiple_of is not None else None,
        AllowInfNan(allow_inf_nan) if allow_inf_nan is not None else None,
    ]


PositiveFloat = Annotated[float, annotated_types.Gt(0)]
NegativeFloat = Annotated[float, annotated_types.Lt(0)]
NonPositiveFloat = Annotated[float, annotated_types.Le(0)]
NonNegativeFloat = Annotated[float, annotated_types.Ge(0)]
StrictFloat = Annotated[float, Strict(True)]
FiniteFloat = Annotated[float, AllowInfNan(False)]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ BYTES TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


def conbytes(
    *,
    min_length: int | None = None,
    max_length: int | None = None,
    strict: bool | None = None,
) -> type[bytes]:
    return Annotated[  # type: ignore[return-value]
        bytes,
        Strict(strict) if strict is not None else None,
        annotated_types.Len(min_length or 0, max_length),
    ]


StrictBytes = Annotated[bytes, Strict()]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ STRING TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


def constr(
    *,
    strip_whitespace: bool | None = None,
    to_upper: bool | None = None,
    to_lower: bool | None = None,
    strict: bool | None = None,
    min_length: int | None = None,
    max_length: int | None = None,
    pattern: str | None = None,
) -> type[str]:
    return Annotated[  # type: ignore[return-value]
        str,
        Strict(strict) if strict is not None else None,
        annotated_types.Len(min_length or 0, max_length),
        _fields.PydanticGeneralMetadata(
            strip_whitespace=strip_whitespace,
            to_upper=to_upper,
            to_lower=to_lower,
            pattern=pattern,
        ),
    ]


StrictStr = Annotated[str, Strict()]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~ COLLECTION TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
HashableItemType = TypeVar('HashableItemType', bound=Hashable)


def conset(
    item_type: Type[HashableItemType], *, min_length: int = None, max_length: int = None
) -> Type[Set[HashableItemType]]:
    return Annotated[  # type: ignore[return-value]
        Set[item_type], annotated_types.Len(min_length or 0, max_length)  # type: ignore[valid-type]
    ]


def confrozenset(
    item_type: Type[HashableItemType], *, min_length: int | None = None, max_length: int | None = None
) -> Type[FrozenSet[HashableItemType]]:
    return Annotated[  # type: ignore[return-value]
        FrozenSet[item_type],  # type: ignore[valid-type]
        annotated_types.Len(min_length or 0, max_length),
    ]


AnyItemType = TypeVar('AnyItemType')


def conlist(
    item_type: Type[AnyItemType], *, min_length: int | None = None, max_length: int | None = None
) -> Type[List[AnyItemType]]:
    return Annotated[  # type: ignore[return-value]
        List[item_type],  # type: ignore[valid-type]
        annotated_types.Len(min_length or 0, max_length),
    ]


def contuple(
    item_type: Type[AnyItemType], *, min_length: int | None = None, max_length: int | None = None
) -> Type[Tuple[AnyItemType]]:
    return Annotated[  # type: ignore[return-value]
        Tuple[item_type],
        annotated_types.Len(min_length or 0, max_length),
    ]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~ IMPORT STRING TYPE ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

AnyType = TypeVar('AnyType')
if TYPE_CHECKING:
    ImportString = Annotated[AnyType, ...]
else:

    class ImportString:
        @classmethod
        def __class_getitem__(cls, item: AnyType) -> AnyType:
            return Annotated[item, cls()]

        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.CoreSchema:
            if schema is None or schema == {'type': 'any'}:
                # Treat bare usage of ImportString (`schema is None`) as the same as ImportString[Any]
                return core_schema.function_plain_schema(lambda v, _: _validators.import_string(v))
            else:
                return core_schema.function_before_schema(lambda v, _: _validators.import_string(v), schema)

        def __repr__(self) -> str:
            return 'ImportString'


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ DECIMAL TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


def condecimal(
    *,
    strict: bool | None = None,
    gt: int | Decimal | None = None,
    ge: int | Decimal | None = None,
    lt: int | Decimal | None = None,
    le: int | Decimal | None = None,
    multiple_of: int | Decimal | None = None,
    max_digits: int | None = None,
    decimal_places: int | None = None,
    allow_inf_nan: bool | None = None,
) -> Type[Decimal]:
    return Annotated[  # type: ignore[return-value]
        Decimal,
        Strict(strict) if strict is not None else None,
        annotated_types.Interval(gt=gt, ge=ge, lt=lt, le=le),
        annotated_types.MultipleOf(multiple_of) if multiple_of is not None else None,
        _fields.PydanticGeneralMetadata(max_digits=max_digits, decimal_places=decimal_places),
        AllowInfNan(allow_inf_nan) if allow_inf_nan is not None else None,
    ]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ UUID TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


@_dataclasses.dataclass(frozen=True)  # Add frozen=True to make it hashable
class UuidVersion:
    uuid_version: Literal[1, 3, 4, 5]

    def __pydantic_modify_json_schema__(self, field_schema: dict[str, Any]) -> None:
        field_schema.pop('anyOf', None)  # remove the bytes/str union
        field_schema.update(type='string', format=f'uuid{self.uuid_version}')

    def __get_pydantic_core_schema__(
        self, schema: core_schema.CoreSchema, **_kwargs: Any
    ) -> core_schema.FunctionSchema:
        return core_schema.function_after_schema(schema, cast(core_schema.ValidatorFunction, self.validate))

    def validate(self, value: UUID, _: core_schema.ValidationInfo) -> UUID:
        if value.version != self.uuid_version:
            raise PydanticCustomError(
                'uuid_version', 'uuid version {required_version} expected', {'required_version': self.uuid_version}
            )
        return value


UUID1 = Annotated[UUID, UuidVersion(1)]
UUID3 = Annotated[UUID, UuidVersion(3)]
UUID4 = Annotated[UUID, UuidVersion(4)]
UUID5 = Annotated[UUID, UuidVersion(5)]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ PATH TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


@_dataclasses.dataclass
class PathType:
    path_type: Literal['file', 'dir', 'new']

    def __pydantic_modify_json_schema__(self, field_schema: dict[str, Any]) -> None:
        format_conversion = {'file': 'file-path', 'dir': 'directory-path'}
        field_schema.update(format=format_conversion.get(self.path_type, 'path'), type='string')

    def __get_pydantic_core_schema__(
        self, schema: core_schema.CoreSchema, **_kwargs: Any
    ) -> core_schema.FunctionSchema:
        function_lookup = {
            'file': cast(core_schema.ValidatorFunction, self.validate_file),
            'dir': cast(core_schema.ValidatorFunction, self.validate_directory),
            'new': cast(core_schema.ValidatorFunction, self.validate_new),
        }

        return core_schema.function_after_schema(
            schema,
            function_lookup[self.path_type],
        )

    @staticmethod
    def validate_file(path: Path, _: core_schema.ValidationInfo) -> Path:
        if path.is_file():
            return path
        else:
            raise PydanticCustomError('path_not_file', 'Path does not point to a file')

    @staticmethod
    def validate_directory(path: Path, _: core_schema.ValidationInfo) -> Path:
        if path.is_dir():
            return path
        else:
            raise PydanticCustomError('path_not_directory', 'Path does not point to a directory')

    @staticmethod
    def validate_new(path: Path, _: core_schema.ValidationInfo) -> Path:
        if path.exists():
            raise PydanticCustomError('path_exists', 'path already exists')
        elif not path.parent.exists():
            raise PydanticCustomError('parent_does_not_exist', 'Parent directory does not exist')
        else:
            return path


FilePath = Annotated[Path, PathType('file')]
DirectoryPath = Annotated[Path, PathType('dir')]
NewPath = Annotated[Path, PathType('new')]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ JSON TYPE ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

if TYPE_CHECKING:
    Json = Annotated[AnyType, ...]  # Json[list[str]] will be recognized by type checkers as list[str]

else:

    class Json:
        @classmethod
        def __class_getitem__(cls, item: AnyType) -> AnyType:
            return Annotated[item, cls()]

        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.JsonSchema:
            return core_schema.json_schema(schema)

        @classmethod
        def __pydantic_modify_json_schema__(cls, field_schema: dict[str, Any]) -> None:
            field_schema.update(type='string', format='json-string')

        def __repr__(self) -> str:
            return 'Json'

        def __hash__(self) -> int:
            return hash(type(self))

        def __eq__(self, other: Any) -> bool:
            return type(other) == type(self)


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ SECRET TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

SecretType = TypeVar('SecretType', str, bytes)


class SecretField(abc.ABC, Generic[SecretType]):
    _error_kind: str

    def __init__(self, secret_value: SecretType) -> None:
        self._secret_value: SecretType = secret_value

    def get_secret_value(self) -> SecretType:
        return self._secret_value

    @classmethod
    def __get_pydantic_core_schema__(cls, **_kwargs: Any) -> core_schema.FunctionSchema:
        validator = SecretFieldValidator(cls)
        if issubclass(cls, SecretStr):
            # Use a lambda here so that `apply_metadata` can be called on the validator before the override is generated
            override = lambda: core_schema.str_schema(  # noqa E731
                min_length=validator.min_length,
                max_length=validator.max_length,
            )
        elif issubclass(cls, SecretBytes):
            override = lambda: core_schema.bytes_schema(  # noqa E731
                min_length=validator.min_length,
                max_length=validator.max_length,
            )
        else:
            override = None
        metadata = build_metadata_dict(
            update_cs_function=validator.__pydantic_update_schema__,
            js_metadata=JsonSchemaMetadata(core_schema_override=override),
        )
        return core_schema.function_after_schema(
            core_schema.union_schema(
                core_schema.is_instance_schema(cls),
                cls._pre_core_schema(),
                strict=True,
                custom_error_type=cls._error_kind,
            ),
            validator,
            metadata=metadata,
            serialization=core_schema.function_plain_ser_schema(cls._serialize, json_return_type='str'),
        )

    @classmethod
    def _serialize(
        cls, value: SecretField[SecretType], info: core_schema.SerializationInfo
    ) -> str | SecretField[SecretType]:
        if info.mode == 'json':
            # we want the output to always be string without the `b'` prefix for byties,
            # hence we just use `secret_display`
            return secret_display(value)
        else:
            return value

    @classmethod
    @abc.abstractmethod
    def _pre_core_schema(cls) -> core_schema.CoreSchema:
        ...

    @classmethod
    def __pydantic_modify_json_schema__(cls, field_schema: dict[str, Any]) -> None:
        update_not_none(
            field_schema,
            type='string',
            writeOnly=True,
            format='password',
        )

    def __eq__(self, other: Any) -> bool:
        return isinstance(other, self.__class__) and self.get_secret_value() == other.get_secret_value()

    def __hash__(self) -> int:
        return hash(self.get_secret_value())

    def __len__(self) -> int:
        return len(self._secret_value)

    @abc.abstractmethod
    def _display(self) -> SecretType:
        ...

    def __str__(self) -> str:
        return str(self._display())

    def __repr__(self) -> str:
        return f'{self.__class__.__name__}({self._display()!r})'


def secret_display(secret_field: SecretField[Any]) -> str:
    return '**********' if secret_field.get_secret_value() else ''


class SecretFieldValidator(_fields.CustomValidator, Generic[SecretType]):
    __slots__ = 'field_type', 'min_length', 'max_length', 'error_prefix'

    def __init__(
        self, field_type: Type[SecretField[SecretType]], min_length: int | None = None, max_length: int | None = None
    ) -> None:
        self.field_type: Type[SecretField[SecretType]] = field_type
        self.min_length = min_length
        self.max_length = max_length
        self.error_prefix: Literal['string', 'bytes'] = 'string' if field_type is SecretStr else 'bytes'

    def __call__(self, __value: SecretField[SecretType] | SecretType, _: core_schema.ValidationInfo) -> Any:
        if self.min_length is not None and len(__value) < self.min_length:
            short_kind: core_schema.ErrorType = f'{self.error_prefix}_too_short'  # type: ignore[assignment]
            raise PydanticKnownError(short_kind, {'min_length': self.min_length})
        if self.max_length is not None and len(__value) > self.max_length:
            long_kind: core_schema.ErrorType = f'{self.error_prefix}_too_long'  # type: ignore[assignment]
            raise PydanticKnownError(long_kind, {'max_length': self.max_length})

        if isinstance(__value, self.field_type):
            return __value
        else:
            return self.field_type(__value)  # type: ignore[arg-type]

    def __pydantic_update_schema__(self, schema: core_schema.CoreSchema, **constraints: Any) -> None:
        self._update_attrs(constraints, {'min_length', 'max_length'})


class SecretStr(SecretField[str]):
    _error_kind = 'string_type'

    @classmethod
    def _pre_core_schema(cls) -> core_schema.CoreSchema:
        return core_schema.str_schema()

    def _display(self) -> str:
        return secret_display(self)


class SecretBytes(SecretField[bytes]):
    _error_kind = 'bytes_type'

    @classmethod
    def _pre_core_schema(cls) -> core_schema.CoreSchema:
        return core_schema.bytes_schema()

    def _display(self) -> bytes:
        return secret_display(self).encode()


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ PAYMENT CARD TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~


class PaymentCardBrand(str, Enum):
    # If you add another card type, please also add it to the
    # Hypothesis strategy in `pydantic._hypothesis_plugin`.
    amex = 'American Express'
    mastercard = 'Mastercard'
    visa = 'Visa'
    other = 'other'

    def __str__(self) -> str:
        return self.value


class PaymentCardNumber(str):
    """
    Based on: https://en.wikipedia.org/wiki/Payment_card_number
    """

    strip_whitespace: ClassVar[bool] = True
    min_length: ClassVar[int] = 12
    max_length: ClassVar[int] = 19
    bin: str
    last4: str
    brand: PaymentCardBrand

    def __init__(self, card_number: str):
        self.validate_digits(card_number)

        card_number = self.validate_luhn_check_digit(card_number)

        self.bin = card_number[:6]
        self.last4 = card_number[-4:]
        self.brand = self.validate_brand(card_number)

    @classmethod
    def __get_pydantic_core_schema__(cls, **_kwargs: Any) -> core_schema.FunctionSchema:
        return core_schema.function_after_schema(
            core_schema.str_schema(
                min_length=cls.min_length, max_length=cls.max_length, strip_whitespace=cls.strip_whitespace
            ),
            cls.validate,
        )

    @classmethod
    def validate(cls, __input_value: str, _: core_schema.ValidationInfo) -> 'PaymentCardNumber':
        return cls(__input_value)

    @property
    def masked(self) -> str:
        num_masked = len(self) - 10  # len(bin) + len(last4) == 10
        return f'{self.bin}{"*" * num_masked}{self.last4}'

    @classmethod
    def validate_digits(cls, card_number: str) -> None:
        if not card_number.isdigit():
            raise PydanticCustomError('payment_card_number_digits', 'Card number is not all digits')

    @classmethod
    def validate_luhn_check_digit(cls, card_number: str) -> str:
        """
        Based on: https://en.wikipedia.org/wiki/Luhn_algorithm
        """
        sum_ = int(card_number[-1])
        length = len(card_number)
        parity = length % 2
        for i in range(length - 1):
            digit = int(card_number[i])
            if i % 2 == parity:
                digit *= 2
            if digit > 9:
                digit -= 9
            sum_ += digit
        valid = sum_ % 10 == 0
        if not valid:
            raise PydanticCustomError('payment_card_number_luhn', 'Card number is not luhn valid')
        return card_number

    @staticmethod
    def validate_brand(card_number: str) -> PaymentCardBrand:
        """
        Validate length based on BIN for major brands:
        https://en.wikipedia.org/wiki/Payment_card_number#Issuer_identification_number_(IIN)
        """
        if card_number[0] == '4':
            brand = PaymentCardBrand.visa
        elif 51 <= int(card_number[:2]) <= 55:
            brand = PaymentCardBrand.mastercard
        elif card_number[:2] in {'34', '37'}:
            brand = PaymentCardBrand.amex
        else:
            brand = PaymentCardBrand.other

        required_length: Union[None, int, str] = None
        if brand in PaymentCardBrand.mastercard:
            required_length = 16
            valid = len(card_number) == required_length
        elif brand == PaymentCardBrand.visa:
            required_length = '13, 16 or 19'
            valid = len(card_number) in {13, 16, 19}
        elif brand == PaymentCardBrand.amex:
            required_length = 15
            valid = len(card_number) == required_length
        else:
            valid = True

        if not valid:
            raise PydanticCustomError(
                'payment_card_number_brand',
                'Length for a {brand} card must be {required_length}',
                {'brand': brand, 'required_length': required_length},
            )
        return brand


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ BYTE SIZE TYPE ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

BYTE_SIZES = {
    'b': 1,
    'kb': 10**3,
    'mb': 10**6,
    'gb': 10**9,
    'tb': 10**12,
    'pb': 10**15,
    'eb': 10**18,
    'kib': 2**10,
    'mib': 2**20,
    'gib': 2**30,
    'tib': 2**40,
    'pib': 2**50,
    'eib': 2**60,
}
BYTE_SIZES.update({k.lower()[0]: v for k, v in BYTE_SIZES.items() if 'i' not in k})
byte_string_re = re.compile(r'^\s*(\d*\.?\d+)\s*(\w+)?', re.IGNORECASE)


class ByteSize(int):
    @classmethod
    def __get_pydantic_core_schema__(cls, **_kwargs: Any) -> core_schema.FunctionPlainSchema:
        # TODO better schema
        return core_schema.function_plain_schema(cls.validate)

    @classmethod
    def validate(cls, __input_value: Any, _: core_schema.ValidationInfo) -> 'ByteSize':
        try:
            return cls(int(__input_value))
        except ValueError:
            pass

        str_match = byte_string_re.match(str(__input_value))
        if str_match is None:
            raise PydanticCustomError('byte_size', 'could not parse value and unit from byte string')

        scalar, unit = str_match.groups()
        if unit is None:
            unit = 'b'

        try:
            unit_mult = BYTE_SIZES[unit.lower()]
        except KeyError:
            raise PydanticCustomError('byte_size_unit', 'could not interpret byte unit: {unit}', {'unit': unit})

        return cls(int(float(scalar) * unit_mult))

    def human_readable(self, decimal: bool = False) -> str:
        if decimal:
            divisor = 1000
            units = 'B', 'KB', 'MB', 'GB', 'TB', 'PB'
            final_unit = 'EB'
        else:
            divisor = 1024
            units = 'B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'
            final_unit = 'EiB'

        num = float(self)
        for unit in units:
            if abs(num) < divisor:
                if unit == 'B':
                    return f'{num:0.0f}{unit}'
                else:
                    return f'{num:0.1f}{unit}'
            num /= divisor

        return f'{num:0.1f}{final_unit}'

    def to(self, unit: str) -> float:
        try:
            unit_div = BYTE_SIZES[unit.lower()]
        except KeyError:
            raise PydanticCustomError('byte_size_unit', 'Could not interpret byte unit: {unit}', {'unit': unit})

        return self / unit_div


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ DATE TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

if TYPE_CHECKING:
    PastDate = Annotated[date, ...]
    FutureDate = Annotated[date, ...]
else:

    class PastDate:
        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.CoreSchema:
            if schema is None:
                # used directly as a type
                return core_schema.date_schema(now_op='past')
            else:
                assert schema['type'] == 'date'
                schema['now_op'] = 'past'
                return schema

        def __repr__(self) -> str:
            return 'PastDate'

    class FutureDate:
        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.CoreSchema:
            if schema is None:
                # used directly as a type
                return core_schema.date_schema(now_op='future')
            else:
                assert schema['type'] == 'date'
                schema['now_op'] = 'future'
                return schema

        def __repr__(self) -> str:
            return 'FutureDate'


def condate(*, strict: bool = None, gt: date = None, ge: date = None, lt: date = None, le: date = None) -> type[date]:
    return Annotated[  # type: ignore[return-value]
        date,
        Strict(strict) if strict is not None else None,
        annotated_types.Interval(gt=gt, ge=ge, lt=lt, le=le),
    ]


# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ DATETIME TYPES ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

if TYPE_CHECKING:
    AwareDatetime = Annotated[datetime, ...]
    NaiveDatetime = Annotated[datetime, ...]
else:

    class AwareDatetime:
        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.CoreSchema:
            if schema is None:
                # used directly as a type
                return core_schema.datetime_schema(tz_constraint='aware')
            else:
                assert schema['type'] == 'datetime'
                schema['tz_constraint'] = 'aware'
                return schema

        def __repr__(self) -> str:
            return 'AwareDatetime'

    class NaiveDatetime:
        @classmethod
        def __get_pydantic_core_schema__(
            cls, schema: core_schema.CoreSchema | None = None, **_kwargs: Any
        ) -> core_schema.CoreSchema:
            if schema is None:
                # used directly as a type
                return core_schema.datetime_schema(tz_constraint='naive')
            else:
                assert schema['type'] == 'datetime'
                schema['tz_constraint'] = 'naive'
                return schema

        def __repr__(self) -> str:
            return 'NaiveDatetime'
