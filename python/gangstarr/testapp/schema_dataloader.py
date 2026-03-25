import json
from collections import defaultdict

import graphene
from aiodataloader import DataLoader
from asgiref.sync import sync_to_async
from django.http import JsonResponse
from graphene_django import DjangoObjectType
from graphql import graphql as graphql_async

from .models import Album, Artist, Track


# ---------------------------------------------------------------------------
# DataLoaders — one query per relationship, batched across all parents
# ---------------------------------------------------------------------------

class AlbumsByArtistLoader(DataLoader):
    async def batch_load_fn(self, artist_ids):
        albums = await sync_to_async(
            lambda: list(Album.objects.filter(artist_id__in=artist_ids))
        )()
        by_artist = defaultdict(list)
        for album in albums:
            by_artist[album.artist_id].append(album)
        return [by_artist.get(aid, []) for aid in artist_ids]


class TracksByAlbumLoader(DataLoader):
    async def batch_load_fn(self, album_ids):
        tracks = await sync_to_async(
            lambda: list(Track.objects.filter(album_id__in=album_ids))
        )()
        by_album = defaultdict(list)
        for track in tracks:
            by_album[track.album_id].append(track)
        return [by_album.get(aid, []) for aid in album_ids]


# ---------------------------------------------------------------------------
# GraphQL types — async resolvers that delegate to the loaders
# ---------------------------------------------------------------------------

class ArtistType(DjangoObjectType):
    class Meta:
        model = Artist
        fields = ("id", "name")

    albums = graphene.List(lambda: AlbumType)

    async def resolve_albums(self, info):
        return await info.context.loaders["albums_by_artist"].load(self.id)


class AlbumType(DjangoObjectType):
    class Meta:
        model = Album
        fields = ("id", "title", "artist")

    tracks = graphene.List(lambda: TrackType)

    async def resolve_tracks(self, info):
        return await info.context.loaders["tracks_by_album"].load(self.id)


class TrackType(DjangoObjectType):
    class Meta:
        model = Track
        fields = ("id", "name", "album", "milliseconds", "unit_price")


# ---------------------------------------------------------------------------
# Query root
# ---------------------------------------------------------------------------

class Query(graphene.ObjectType):
    all_artists = graphene.List(ArtistType, limit=graphene.Int(default_value=10))
    artists_with_albums_and_tracks = graphene.List(ArtistType, limit=graphene.Int(default_value=10))

    async def resolve_all_artists(self, info, limit=10):
        return await sync_to_async(lambda: list(Artist.objects.all()[:limit]))()

    async def resolve_artists_with_albums_and_tracks(self, info, limit=10):
        return await sync_to_async(lambda: list(Artist.objects.all()[:limit]))()


dataloader_schema = graphene.Schema(query=Query)


# ---------------------------------------------------------------------------
# Lightweight async Django view — uses graphql-core-3's async executor so
# all resolvers share the same event loop and DataLoader batching works.
# ---------------------------------------------------------------------------

class _DataLoaderContext:
    """Per-request context carrying fresh DataLoader instances."""

    def __init__(self):
        self.loaders = {
            "albums_by_artist": AlbumsByArtistLoader(),
            "tracks_by_album": TracksByAlbumLoader(),
        }


async def graphql_dataloader_view(request):
    if request.method == "POST":
        body = json.loads(request.body)
        query = body.get("query", "")
        variables = body.get("variables")
    else:
        query = request.GET.get("query", "")
        variables = None

    result = await graphql_async(
        dataloader_schema.graphql_schema,
        source=query,
        context_value=_DataLoaderContext(),
        variable_values=variables,
    )

    response = {"data": result.data}
    if result.errors:
        response["errors"] = [str(e) for e in result.errors]
    return JsonResponse(response)
