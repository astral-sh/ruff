from django import forms


class TestModelForm1(forms.ModelForm):
    class Meta:
        exclude = ["bar"]


class TestModelForm2(forms.ModelForm):
    class Meta:
        fields = ["foo"]
