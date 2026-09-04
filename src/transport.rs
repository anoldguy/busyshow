use busylib::{BoxFuture, HttpTransport, HttpTransportResult};
use bytes::Bytes;
use http::Request;
use http::header::HeaderValue;

/// Adds the `x-api-token` header a bar wants over Wi-Fi; `busylib` only
/// knows the cloud's bearer token.
pub struct LocalToken<T> {
    pub inner: T,
    pub token: Option<HeaderValue>,
}

impl<T: HttpTransport> HttpTransport for LocalToken<T> {
    fn execute(&self, mut request: Request<Bytes>) -> BoxFuture<'_, HttpTransportResult> {
        if let Some(token) = &self.token {
            request.headers_mut().insert("x-api-token", token.clone());
        }
        self.inner.execute(request)
    }
}
