from django import forms


class TestModelForm1(forms.ModelForm):
    class Meta:
        fields = "__all__"


class TestModelForm2(forms.ModelForm):
    class Meta:
        fields = b"__all__"


class TestModelForm3(forms.ModelForm):
    class Meta:
        fields = ["foo"]
