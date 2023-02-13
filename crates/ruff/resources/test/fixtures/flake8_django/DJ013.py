from django.db.models.signals import pre_save
from django.dispatch import receiver
from myapp.models import MyModel

test_decorator = lambda func: lambda *args, **kwargs: func(*args, **kwargs)


@receiver(pre_save, sender=MyModel)
@test_decorator
def correct_pre_save_handler():
    pass


@test_decorator
@receiver(pre_save, sender=MyModel)
def incorrect_pre_save_handler():
    pass


@receiver(pre_save, sender=MyModel)
@receiver(pre_save, sender=MyModel)
@test_decorator
def correct_multiple():
    pass


@receiver(pre_save, sender=MyModel)
@receiver(pre_save, sender=MyModel)
def correct_multiple():
    pass


@receiver(pre_save, sender=MyModel)
@test_decorator
@receiver(pre_save, sender=MyModel)
def incorrect_multiple():
    pass
