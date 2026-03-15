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
        'ENGINE': 'django.db.backends.sqlite3',
        'NAME': BASE_DIR / 'db.sqlite3',
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
