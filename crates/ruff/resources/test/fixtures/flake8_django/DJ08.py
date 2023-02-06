class IncorrectModelWithAttr(models.Model):
    pass


class IncorrectModel(Model):
    pass


class CorrectModel(Model):
    def __str__(self) -> str:
        return "This is model"


class IncorrectButAbstract(models.Model):
    class Meta:
        abstract = True
