from gangstarr.testapp.models import Artist, Album


def create_albums():
    artist = Artist.objects.create(name='Gang Starr')
    for i in range(5):
        Album.objects.create(title=f'Album {i}', artist=artist)


def get_artists() -> list[str]:
    artists = []
    albums = Album.objects.all()
    for album in albums:
        artists.append(album.artist.name)
    return artists
