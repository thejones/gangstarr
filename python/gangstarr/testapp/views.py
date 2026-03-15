from django.http import JsonResponse
from django.shortcuts import get_object_or_404, render

from .models import Artist
from .my_module import get_artists


def artists_view(request):
    artists = get_artists()
    return JsonResponse({'artists': artists})


def home(request):
    artist_count = Artist.objects.count()
    return render(request, 'testapp/home.html', {'artist_count': artist_count})


def artist_detail(request, artist_id):
    artist = get_object_or_404(Artist, pk=artist_id)
    albums = artist.albums.all()
    return render(request, 'testapp/artist_detail.html', {'artist': artist, 'albums': albums})
