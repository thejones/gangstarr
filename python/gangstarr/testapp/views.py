from django.http import JsonResponse

from gangstarr.testapp.my_module import get_artists


def artists_view(request):
    artists = get_artists()
    return JsonResponse({'artists': artists})
