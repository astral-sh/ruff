import sys
from typing import Literal

if sys.version_info >= (3, 14):
    import enum
    import types
    from _typeshed import AnnotateFunc, AnnotationForm, EvaluateFunc, SupportsItems
    from collections.abc import Mapping
    from typing import Any, ParamSpec, TypeVar, TypeVarTuple, final, overload
    from warnings import deprecated

    __all__ = [
        "Format",
        "ForwardRef",
        "call_annotate_function",
        "call_evaluate_function",
        "get_annotate_from_class_namespace",
        "get_annotations",
        "annotations_to_string",
        "type_repr",
    ]

    class Format(enum.IntEnum):
        VALUE = 1
        VALUE_WITH_FAKE_GLOBALS = 2
        FORWARDREF = 3
        STRING = 4

    @final
    class ForwardRef:
        __slots__ = (
            "__forward_is_argument__",
            "__forward_is_class__",
            "__forward_module__",
            "__weakref__",
            "__arg__",
            "__globals__",
            "__extra_names__",
            "__code__",
            "__ast_node__",
            "__cell__",
            "__owner__",
            "__stringifier_dict__",
        )
        __forward_is_argument__: bool
        __forward_is_class__: bool
        __forward_module__: str | None
        def __init__(
            self, arg: str, *, module: str | None = None, owner: object = None, is_argument: bool = True, is_class: bool = False
        ) -> None: ...
        @overload
        def evaluate(
            self,
            *,
            globals: dict[str, Any] | None = None,
            locals: Mapping[str, Any] | None = None,
            type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] | None = None,
            owner: object = None,
            format: Literal[Format.STRING],
        ) -> str: ...
        @overload
        def evaluate(
            self,
            *,
            globals: dict[str, Any] | None = None,
            locals: Mapping[str, Any] | None = None,
            type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] | None = None,
            owner: object = None,
            format: Literal[Format.FORWARDREF],
        ) -> AnnotationForm | ForwardRef: ...
        @overload
        def evaluate(
            self,
            *,
            globals: dict[str, Any] | None = None,
            locals: Mapping[str, Any] | None = None,
            type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] | None = None,
            owner: object = None,
            format: Format = Format.VALUE,  # noqa: Y011
        ) -> AnnotationForm: ...
        @deprecated("Use `ForwardRef.evaluate()` or `typing.evaluate_forward_ref()` instead.")
        def _evaluate(
            self,
            globalns: dict[str, Any] | None,
            localns: Mapping[str, Any] | None,
            type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] = ...,
            *,
            recursive_guard: frozenset[str],
        ) -> AnnotationForm: ...
        @property
        def __forward_arg__(self) -> str: ...
        @property
        def __forward_code__(self) -> types.CodeType: ...
        def __eq__(self, other: object) -> bool: ...
        def __hash__(self) -> int: ...
        def __or__(self, other: Any) -> types.UnionType: ...
        def __ror__(self, other: Any) -> types.UnionType: ...

    @overload
    def call_evaluate_function(evaluate: EvaluateFunc, format: Literal[Format.STRING], *, owner: object = None) -> str: ...
    @overload
    def call_evaluate_function(
        evaluate: EvaluateFunc, format: Literal[Format.FORWARDREF], *, owner: object = None
    ) -> AnnotationForm | ForwardRef: ...
    @overload
    def call_evaluate_function(evaluate: EvaluateFunc, format: Format, *, owner: object = None) -> AnnotationForm: ...
    @overload
    def call_annotate_function(
        annotate: AnnotateFunc, format: Literal[Format.STRING], *, owner: object = None
    ) -> dict[str, str]: ...
    @overload
    def call_annotate_function(
        annotate: AnnotateFunc, format: Literal[Format.FORWARDREF], *, owner: object = None
    ) -> dict[str, AnnotationForm | ForwardRef]: ...
    @overload
    def call_annotate_function(annotate: AnnotateFunc, format: Format, *, owner: object = None) -> dict[str, AnnotationForm]: ...
    def get_annotate_from_class_namespace(obj: Mapping[str, object]) -> AnnotateFunc | None: ...
    @overload
    def get_annotations(
        obj: Any,  # any object with __annotations__ or __annotate__
        *,
        globals: dict[str, object] | None = None,
        locals: Mapping[str, object] | None = None,
        eval_str: bool = False,
        format: Literal[Format.STRING],
    ) -> dict[str, str]: ...
    @overload
    def get_annotations(
        obj: Any,
        *,
        globals: dict[str, object] | None = None,
        locals: Mapping[str, object] | None = None,
        eval_str: bool = False,
        format: Literal[Format.FORWARDREF],
    ) -> dict[str, AnnotationForm | ForwardRef]: ...
    @overload
    def get_annotations(
        obj: Any,
        *,
        globals: dict[str, object] | None = None,
        locals: Mapping[str, object] | None = None,
        eval_str: bool = False,
        format: Format = Format.VALUE,  # noqa: Y011
    ) -> dict[str, AnnotationForm]: ...
    def type_repr(value: object) -> str: ...
    def annotations_to_string(annotations: SupportsItems[str, object]) -> dict[str, str]: ...
