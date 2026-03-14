from pathlib import Path

import gangstarr

BASE_DIR = Path(__file__).resolve().parent

INSTALLED_APPS = [
    'django.contrib.auth',
    'django.contrib.contenttypes',
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

MIDDLEWARE = ['gangstarr.middleware.MomentOfTruthMiddleware']

GANGSTAR_BASE_DIR = gangstarr.default_base_dir(__file__)
