import contextlib

from django.conf import settings
from django.db import connection

from gangstarr.reporting import Guru, PrintingOptions, ReportingOptions

from .premier import Premier


class full_clip(contextlib.ContextDecorator):
    def __init__(self, reporting_options: ReportingOptions = None, meta_data: dict[str, str] = None):
        self.meta_data = meta_data
        if reporting_options is None:
            try:
                self._reporting_options = settings.GANGSTAR_REPORTING_OPTIONS
            except AttributeError:
                self._reporting_options = PrintingOptions()
        else:
            self._reporting_options = reporting_options

        self._premier = Premier(reporting_options=self._reporting_options, meta_data=self.meta_data)
        self.query_info = self._premier.query_info
        self.reporter = Guru.create(premier=self._premier)
        self._pre_execute_hook = connection.execute_wrapper(self._premier)

    def __enter__(self):
        self._pre_execute_hook.__enter__()
        return self

    def __exit__(self, *exc):
        try:
            self.reporter.report()
        finally:
            self._pre_execute_hook.__exit__(*exc)
