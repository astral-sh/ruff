def f():
    from django.contrib.auth.models import AbstractBaseUser

    def test_remove_inner_quotes_double(self, user: AbstractBaseUser["int"]):
        pass


def f():
    from django.contrib.auth.models import AbstractBaseUser

    def test_remove_inner_quotes_single(self, user: AbstractBaseUser['int']):
        pass


def f():
    from django.contrib.auth.models import AbstractBaseUser

    def test_remove_inner_quotes_mixed(self, user: AbstractBaseUser['int', "str"]):
        pass


def f():
    from typing import Annotated, Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_literal_annotation_args_contain_quotes(self, type1: AbstractBaseUser[Literal["user", "admin"], Annotated["int", "1", 2]]):
        pass


def f():
    from typing import Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_union_contain_inner_quotes(self, type1: AbstractBaseUser["int" | Literal["int"]]):
        pass


def f():
    from typing import Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_inner_literal_mixed_quotes(user: AbstractBaseUser[Literal['user', "admin"]]):
        pass


def f():
    from typing import Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_inner_literal_single_quote(user: AbstractBaseUser[Literal['int'], str]):
        pass


def f():
    from typing import Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_mixed_quotes_literal(user: AbstractBaseUser[Literal['user'], "int"]):
        pass


def f():
    from typing import Annotated, Literal

    from django.contrib.auth.models import AbstractBaseUser

    def test_annotated_literal_mixed_quotes(user: AbstractBaseUser[Annotated[str, "max_length=20", Literal['user', "admin"]]]):
        pass


def f():
    from typing import Literal, Union

    def test_union_literal_no_removal(arg: Union[Literal["true", "false"], None]):
        pass


def f():
    from typing import Literal, Optional

    def test_optional_literal_no_removal(arg: Optional[Literal["red", "blue"]]):
        pass


def f():
    from typing import Annotated

    from fastapi import Depends

    from .foo import get_foo

    def test_annotated_non_typing_reference(user: Annotated[str, Depends(get_foo)]):
        pass
