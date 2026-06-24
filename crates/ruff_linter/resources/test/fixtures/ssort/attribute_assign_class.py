import admin
class TestAdmin(admin.ModelAdmin):
    list_filter = ("foo_method",)
    def foo_method(self, obj):
        return "something"
    foo_method.short_description = "Foo method"
