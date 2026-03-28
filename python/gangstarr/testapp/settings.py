import os
from pathlib import Path

import gangstarr

BASE_DIR = Path(__file__).resolve().parent

INSTALLED_APPS = [
    'django.contrib.auth',
    'django.contrib.contenttypes',
    'django.contrib.staticfiles',
    'rest_framework',
    'graphene_django',
    'gangstarr',
    'gangstarr.testapp',
]

DATABASES = {
    'default': {
        'ENGINE': 'django.db.backends.postgresql',
        'NAME': os.environ.get('PGDATABASE', 'gangstarr'),
        'USER': os.environ.get('PGUSER', 'gangstarr'),
        'PASSWORD': os.environ.get('PGPASSWORD', 'gangstarr'),
        'HOST': os.environ.get('PGHOST', 'localhost'),
        'PORT': os.environ.get('PGPORT', '5433'),
    }
}

ROOT_URLCONF = 'gangstarr.testapp.urls'
SECRET_KEY = '1234'
DEBUG = True

MIDDLEWARE = [
    # 'gangstarr.middleware.YouKnowMySteezeMiddleware',
    'gangstarr.middleware.MomentOfTruthMiddleware'
]

GANGSTAR_BASE_DIR = gangstarr.default_base_dir(__file__)
GANGSTARR_COLOR_THEME = '505'

GRAPHENE = {
    'MIDDLEWARE': ['gangstarr.graphene.DWYCKMiddleware'],
}

STATIC_URL = '/static/'

TEMPLATES = [
    {
        'BACKEND': 'django.template.backends.django.DjangoTemplates',
        'DIRS': [],
        'APP_DIRS': True,
        'OPTIONS': {
            'context_processors': [
                'django.template.context_processors.request',
            ],
        },
    },
]
