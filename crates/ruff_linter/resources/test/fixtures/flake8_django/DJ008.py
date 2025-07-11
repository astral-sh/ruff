from django.db import models
from django.db.models import Model


# Models without __str__
class TestModel1(models.Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        verbose_name = "test model"
        verbose_name_plural = "test models"

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class TestModel2(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        verbose_name = "test model"
        verbose_name_plural = "test models"

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class TestModel3(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = False

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Models with __str__
class TestModel4(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        verbose_name = "test model"
        verbose_name_plural = "test models"

    def __str__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class TestModel5(models.Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        verbose_name = "test model"
        verbose_name_plural = "test models"

    def __str__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Abstract models without str
class AbstractTestModel1(models.Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = True

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class AbstractTestModel2(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = True

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Abstract models with __str__


class AbstractTestModel3(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = True

    def __str__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class AbstractTestModel4(models.Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = True

    def __str__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


class AbstractTestModel5(models.Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = False

    def __str__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Subclass with its own __str__
class SubclassTestModel1(TestModel1):
    def __str__(self):
        return self.new_field


# Subclass with inherited __str__
class SubclassTestModel2(TestModel4):
    pass


# Subclass without __str__
class SubclassTestModel3(TestModel1):
    pass


# Test cases for type-annotated abstract models - these should NOT trigger DJ008
from typing import ClassVar
from django_stubs_ext.db.models import TypedModelMeta


class TypeAnnotatedAbstractModel1(models.Model):
    """Model with type-annotated abstract = True - should not trigger DJ008"""
    new_field = models.CharField(max_length=10)

    class Meta(TypedModelMeta):
        abstract: ClassVar[bool] = True


class TypeAnnotatedAbstractModel2(models.Model):
    """Model with type-annotated abstract = True using regular Meta - should not trigger DJ008"""  
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract: ClassVar[bool] = True


class TypeAnnotatedAbstractModel3(models.Model):
    """Model with type-annotated abstract = True but without ClassVar - should not trigger DJ008"""
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract: bool = True


class TypeAnnotatedNonAbstractModel(models.Model):
    """Model with type-annotated abstract = False - should trigger DJ008"""
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract: ClassVar[bool] = False


class TypeAnnotatedAbstractModelWithStr(models.Model):
    """Model with type-annotated abstract = True and __str__ method - should not trigger DJ008"""
    new_field = models.CharField(max_length=10)

    class Meta(TypedModelMeta):
        abstract: ClassVar[bool] = True

    def __str__(self):
        return self.new_field
