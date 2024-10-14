def f():
    from django.contrib.auth.models import AbstractBaseUser
    from typing import Annotated

    def test_remove_inner_quotes_double(self, user: AbstractBaseUser["int"], view: "type[CondorBaseViewSet]"):
        pass

    def test_remove_inner_quotes_single(self, user: AbstractBaseUser['int']):
        pass

    def test_remove_inner_quotes_mixed(self, user: AbstractBaseUser['int', "str"]):
        pass

    def test_literal_annotation_args_contain_quotes(self, type1: AbstractBaseUser[Literal["user", "admin"], Annotated["int", "1", 2]]):
        pass

    def test_union_contain_inner_quotes(self, type1: AbstractBaseUser["int" | Literal["int"]]):
        pass

    def test_literal_cannot_remove(user: AbstractBaseUser[Literal['user', "admin"]]):
        pass

    def test_literal_cannot_remove2(user: AbstractBaseUser[Literal['int'], str]):
        pass

    def test_type_literal_mixed_quotes(view: type["CondorBaseViewSet"], role: Literal["admin", 'user']):
        pass

    def test_mixed_quotes_literal(user: AbstractBaseUser[Literal['user'], "int"]):
        pass

    def test_annotated_literal_mixed_quotes(user: Annotated[str, "max_length=20", Literal['user', "admin"]]):
        pass


    def test_type_annotated_mixed(view: type["CondorBaseViewSet"], data: Annotated[int, "min_value=1", Literal["admin", 'superuser']]):
        pass

    def test_union_literal_mixed_quotes(user: Union[Literal['active', "inactive"], str]):
        pass

    def test_callable_literal_mixed_quotes(callable_fn: Callable[["int", Literal['admin', "user"]], 'bool']):
        pass

    def test_callable_annotated_literal(callable_fn: Callable[[int, Annotated[str, Literal['active', "inactive"]]], bool]):
        pass

    def test_generic_alias_literal_mixed_quotes(user: Union["List[int]", Literal['admin', "user"], 'Dict[str, Any]']):
        pass

    def test_literal_no_inner_removal(arg: Literal["user", "admin"]):
        pass

    def test_quoted_annotation_literal(arg: "Literal['user', 'admin']"):
        pass

    def test_annotated_with_expression(arg: Annotated[str, "min_length=3"]):
        pass

    def test_union_literal_no_removal(arg: Union[Literal["true", "false"], None]):
        pass

    def test_optional_literal_no_removal(arg: Optional[Literal["red", "blue"]]):
        pass

    def test_type_alias_with_removal(arg: "List[Dict[str, int]]"):
        pass

    def test_final_literal_no_removal(arg: Final[Literal["open", "close"]]):
        pass

    def test_callable_literal_no_removal(arg: Callable[[], Literal["success", "failure"]]):
        pass

    def test_classvar_union_literal_no_removal(arg: ClassVar[Union[Literal["yes", "no"], None]]):
        pass

    def test_tuple_literal_no_removal(arg: tuple[Literal["apple", "orange"], int]):
        pass

def case_attribute():
    import typing
    import a

    def test_attribute(arg: a.b["int"]):
        ...

    def test_attribute_typing_literal(arg: a[typing.Literal["admin"]]):
        ...
