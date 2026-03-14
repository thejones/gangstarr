from django.test import Client
import pytest
from django.urls import reverse

from gangstarr import full_clip
from gangstarr.reporting import PrintingOptions, LoggingOptions, MassAppealException, RaisingOptions
from gangstarr.testapp.my_module import get_artists, create_albums


@pytest.mark.django_db(transaction=True)
def test_full_clip():
    create_albums()
    with full_clip(meta_data=dict(func='get_artists', username='Paul')) as fc:
        get_artists()
    query_info = fc.query_info
    assert len(query_info) == 1


@pytest.mark.django_db(transaction=True)
def test_moment_of_truth_middleware():
    create_albums()
    client = Client()
    response = client.get(reverse('artists'))
    assert response.status_code == 200


@pytest.mark.django_db(transaction=True)
def test_logging():
    create_albums()
    with full_clip(reporting_options=LoggingOptions()) as fc:
        get_artists()
    query_info = fc.query_info
    assert len(query_info) == 1


@pytest.mark.django_db(transaction=True)
def test_mass_appeal_exception():
    create_albums()
    with pytest.raises(MassAppealException):
        with full_clip(reporting_options=RaisingOptions(count_threshold=1, duration_threshold=0.0)):
            get_artists()
