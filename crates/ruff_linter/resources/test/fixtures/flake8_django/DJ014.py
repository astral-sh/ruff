from django.db import models
from django.db.models import Model


# Models without __unicode__
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


# Models with __unicode__
class TestModel4(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        verbose_name = "test model"
        verbose_name_plural = "test models"

    def __unicode__(self):
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

    def __unicode__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Abstract models without __unicode__
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


# Abstract models with __unicode__
class AbstractTestModel3(Model):
    new_field = models.CharField(max_length=10)

    class Meta:
        abstract = True

    def __unicode__(self):
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

    def __unicode__(self):
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

    def __unicode__(self):
        return self.new_field

    @property
    def my_brand_new_property(self):
        return 1

    def my_beautiful_method(self):
        return 2


# Subclass with its own __unicode__
class SubclassTestModel1(TestModel1):
    def __unicode__(self):
        return self.new_field


# Subclass with inherited __unicode__
class SubclassTestModel2(TestModel4):
    pass


# Subclass without __unicode__
class SubclassTestModel3(TestModel1):
    pass
