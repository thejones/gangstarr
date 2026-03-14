from .context_manager import full_clip


class MomentOfTruthMiddleware:
    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        with full_clip(meta_data=dict(url=request.path, method=request.method)):
            response = self.get_response(request)
        return response
