import graphene
from graphene_django import DjangoObjectType

from .models import Album, Artist, Track


class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ('id', 'name', 'albums')


class AlbumType(DjangoObjectType):
    class Meta:
        model = Album
        fields = ('id', 'title', 'artist', 'tracks')


class TrackType(DjangoObjectType):
    class Meta:
        model = Track
        fields = ('id', 'name', 'album', 'milliseconds', 'unit_price')


class Query(graphene.ObjectType):
    all_artists = graphene.List(ArtistType, limit=graphene.Int(default_value=10))
    artists_with_albums_and_tracks = graphene.List(ArtistType, limit=graphene.Int(default_value=10))

    def resolve_all_artists(self, info, limit=10):
        """Optimized with prefetch_related."""
        return Artist.objects.prefetch_related('albums')[:limit]

    def resolve_artists_with_albums_and_tracks(self, info, limit=10):
        """Intentionally unoptimized N+1 query demo."""
        return Artist.objects.all()[:limit]


schema = graphene.Schema(query=Query)
