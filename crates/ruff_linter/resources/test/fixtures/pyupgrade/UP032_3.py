from django.utils.translation import gettext

long = 'long'
split_to = 'split_to'
gettext(
    'some super {} and complicated string so that the error code '
    'E501 Triggers when this is not {} multi-line'.format(
        long, split_to)
)
