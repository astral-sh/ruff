from django.shortcuts import render


def test_view1(request):
    return render(request, "index.html", locals())


def test_view2(request):
    return render(request, "index.html", context=locals())


def test_view3(request):
    return render(request, "index.html")


def test_view4(request):
    return render(request, "index.html", {})


def test_view5(request):
    return render(request, "index.html", context={})
