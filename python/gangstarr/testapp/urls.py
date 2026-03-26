from django.urls import path
from graphene_django.views import GraphQLView

from . import api_views, views
from .schema import schema
from .schema_dataloader import graphql_dataloader_view

urlpatterns = [
    path('', views.home, name='home'),
    path('artists/', views.artists_view, name='artists'),
    path('artists/<int:artist_id>/', views.artist_detail, name='artist_detail'),
    path('api/artists/', api_views.artist_list_api, name='artist_list_api'),
    path('graphql/', GraphQLView.as_view(graphiql=True, schema=schema), name='graphql'),
    path('graphql-dl/', graphql_dataloader_view, name='graphql_dataloader'),
]
