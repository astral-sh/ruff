from django.db.models import Model as DjangoModel
from django.db import models
from django.db.models import CharField as SmthCharField


class IncorrectModel(models.Model):
    charfield = models.CharField(max_length=255, null=True)
    textfield = models.TextField(max_length=255, null=True)
    slugfield = models.SlugField(max_length=255, null=True)
    emailfield = models.EmailField(max_length=255, null=True)
    filepathfield = models.FilePathField(max_length=255, null=True)
    urlfield = models.URLField(max_length=255, null=True)


class IncorrectModelWithAlias(DjangoModel):
    charfield = DjangoModel.CharField(max_length=255, null=True)
    textfield = SmthCharField(max_length=255, null=True)
    slugfield = models.SlugField(max_length=255, null=True)
    emailfield = models.EmailField(max_length=255, null=True)
    filepathfield = models.FilePathField(max_length=255, null=True)
    urlfield = models.URLField(max_length=255, null=True)


class IncorrectModelWithoutSuperclass:
    charfield = DjangoModel.CharField(max_length=255, null=True)
    textfield = SmthCharField(max_length=255, null=True)
    slugfield = models.SlugField(max_length=255, null=True)
    emailfield = models.EmailField(max_length=255, null=True)
    filepathfield = models.FilePathField(max_length=255, null=True)
    urlfield = models.URLField(max_length=255, null=True)


class CorrectModel(models.Model):
    charfield = models.CharField(max_length=255, null=False, blank=True)
    textfield = models.TextField(max_length=255, null=False, blank=True)
    slugfield = models.SlugField(max_length=255, null=False, blank=True)
    emailfield = models.EmailField(max_length=255, null=False, blank=True)
    filepathfield = models.FilePathField(max_length=255, null=False, blank=True)
    urlfield = models.URLField(max_length=255, null=False, blank=True)

    charfieldu = models.CharField(max_length=255, null=True, blank=True, unique=True)
    textfieldu = models.TextField(max_length=255, null=True, blank=True, unique=True)
    slugfieldu = models.SlugField(max_length=255, null=True, blank=True, unique=True)
    emailfieldu = models.EmailField(max_length=255, null=True, blank=True, unique=True)
    filepathfieldu = models.FilePathField(
        max_length=255, null=True, blank=True, unique=True
    )
    urlfieldu = models.URLField(max_length=255, null=True, blank=True, unique=True)
