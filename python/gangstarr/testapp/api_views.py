from rest_framework.decorators import api_view
from rest_framework.response import Response

from .models import Artist
from .serializers import ArtistSerializer


@api_view(['GET'])
def artist_list_api(request):
    """Intentionally unoptimized N+1 query demo."""
    artists = Artist.objects.all()[:50]
    serializer = ArtistSerializer(artists, many=True)
    return Response(serializer.data)
