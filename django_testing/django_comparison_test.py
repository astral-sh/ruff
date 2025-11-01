"""
Django type checking comparison test.
This file will be tested with mypy+django-stubs, pyright, and ty.
"""

from django.db import models
from django.db.models.manager import Manager


class User(models.Model):
    name = models.CharField(max_length=100)
    email = models.EmailField()
    age = models.IntegerField()

    objects = Manager["User"]()


class Post(models.Model):
    title = models.CharField(max_length=200)
    author = models.ForeignKey(User, on_delete=models.CASCADE)

    objects = Manager["Post"]()


# Test 1: Model class type
reveal_type(User)

# Test 2: .objects manager
reveal_type(User.objects)

# Test 3: Manager methods return QuerySet
all_users = User.objects.all()
reveal_type(all_users)

# Test 4: QuerySet.get() returns model instance
user = User.objects.get(id=1)
reveal_type(user)

# Test 5: Field access on instance
reveal_type(user.name)
reveal_type(user.email)
reveal_type(user.age)

# Test 6: Invalid field access
reveal_type(user.nonexistent)  # Should error

# Test 7: ForeignKey access
post = Post.objects.get(id=1)
reveal_type(post.author)

# Test 8: Invalid manager method
User.objects.nonexistent_method()  # Should error

# Test 9: QuerySet chaining
filtered = User.objects.filter(age=25).exclude(name="test")
reveal_type(filtered)

# Test 10: DoesNotExist exception
try:
    User.objects.get(id=999)
except User.DoesNotExist:
    pass
