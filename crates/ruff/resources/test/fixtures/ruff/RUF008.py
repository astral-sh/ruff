from dataclasses import dataclass, field

KNOWINGLY_MUTABLE_DEFAULT = []


@dataclass()
class A:
    mutable_default: list[int] = []
    without_annotation = []
    ignored_via_comment: list[int] = []  # noqa: RUF008
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)


@dataclass
class B:
    mutable_default: list[int] = []
    without_annotation = []
    ignored_via_comment: list[int] = []  # noqa: RUF008
    correct_code: list[int] = KNOWINGLY_MUTABLE_DEFAULT
    perfectly_fine: list[int] = field(default_factory=list)
