from django.db import models


class Artist(models.Model):
    name = models.CharField(max_length=120, blank=True, null=True)

    class Meta:
        db_table = "testapp_artist"


class Album(models.Model):
    title = models.CharField(max_length=160)
    artist = models.ForeignKey(Artist, on_delete=models.CASCADE, related_name="albums")

    class Meta:
        db_table = "testapp_album"


class Employee(models.Model):
    last_name = models.CharField(max_length=20)
    first_name = models.CharField(max_length=20)
    title = models.CharField(max_length=30, blank=True, null=True)
    reports_to = models.ForeignKey(
        "self", on_delete=models.SET_NULL, blank=True, null=True, related_name="subordinates"
    )
    birth_date = models.DateTimeField(blank=True, null=True)
    hire_date = models.DateTimeField(blank=True, null=True)
    address = models.CharField(max_length=70, blank=True, null=True)
    city = models.CharField(max_length=40, blank=True, null=True)
    state = models.CharField(max_length=40, blank=True, null=True)
    country = models.CharField(max_length=40, blank=True, null=True)
    postal_code = models.CharField(max_length=10, blank=True, null=True)
    phone = models.CharField(max_length=24, blank=True, null=True)
    fax = models.CharField(max_length=24, blank=True, null=True)
    email = models.CharField(max_length=60, blank=True, null=True)

    class Meta:
        db_table = "testapp_employee"


class Customer(models.Model):
    first_name = models.CharField(max_length=40)
    last_name = models.CharField(max_length=20)
    company = models.CharField(max_length=80, blank=True, null=True)
    address = models.CharField(max_length=70, blank=True, null=True)
    city = models.CharField(max_length=40, blank=True, null=True)
    state = models.CharField(max_length=40, blank=True, null=True)
    country = models.CharField(max_length=40, blank=True, null=True)
    postal_code = models.CharField(max_length=10, blank=True, null=True)
    phone = models.CharField(max_length=24, blank=True, null=True)
    fax = models.CharField(max_length=24, blank=True, null=True)
    email = models.CharField(max_length=60)
    support_rep = models.ForeignKey(
        Employee, on_delete=models.SET_NULL, blank=True, null=True, related_name="customers"
    )

    class Meta:
        db_table = "testapp_customer"


class Genre(models.Model):
    name = models.CharField(max_length=120, blank=True, null=True)

    class Meta:
        db_table = "testapp_genre"


class MediaType(models.Model):
    name = models.CharField(max_length=120, blank=True, null=True)

    class Meta:
        db_table = "testapp_mediatype"


class Track(models.Model):
    name = models.CharField(max_length=200)
    album = models.ForeignKey(Album, on_delete=models.SET_NULL, blank=True, null=True, related_name="tracks")
    media_type = models.ForeignKey(MediaType, on_delete=models.CASCADE, related_name="tracks")
    genre = models.ForeignKey(Genre, on_delete=models.SET_NULL, blank=True, null=True, related_name="tracks")
    composer = models.CharField(max_length=220, blank=True, null=True)
    milliseconds = models.IntegerField()
    bytes = models.IntegerField(blank=True, null=True)
    unit_price = models.DecimalField(max_digits=10, decimal_places=2)

    class Meta:
        db_table = "testapp_track"


class Playlist(models.Model):
    name = models.CharField(max_length=120, blank=True, null=True)
    tracks = models.ManyToManyField(Track, related_name="playlists", blank=True)

    class Meta:
        db_table = "testapp_playlist"


class Invoice(models.Model):
    customer = models.ForeignKey(Customer, on_delete=models.CASCADE, related_name="invoices")
    invoice_date = models.DateTimeField()
    billing_address = models.CharField(max_length=70, blank=True, null=True)
    billing_city = models.CharField(max_length=40, blank=True, null=True)
    billing_state = models.CharField(max_length=40, blank=True, null=True)
    billing_country = models.CharField(max_length=40, blank=True, null=True)
    billing_postal_code = models.CharField(max_length=10, blank=True, null=True)
    total = models.DecimalField(max_digits=10, decimal_places=2)

    class Meta:
        db_table = "testapp_invoice"


class InvoiceItem(models.Model):
    invoice = models.ForeignKey(Invoice, on_delete=models.CASCADE, related_name="items")
    track = models.ForeignKey(Track, on_delete=models.CASCADE, related_name="invoice_items")
    unit_price = models.DecimalField(max_digits=10, decimal_places=2)
    quantity = models.IntegerField()

    class Meta:
        db_table = "testapp_invoiceitem"
