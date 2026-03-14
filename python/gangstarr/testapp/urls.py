from django.urls import path
from .views import artists_view

urlpatterns = [
    path('artists/', artists_view, name='artists'),
]
