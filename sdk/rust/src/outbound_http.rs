use http_types::{header::HeaderName, HeaderValue};

use super::http::{Request, Response};

use super::wit::v1::http::{self as spin_http};
use super::wit::v1::http_types as spin_http_types;

/// Error type returned by [`send_request`][crate::outbound_http::send_request]
pub use spin_http_types::HttpError as OutboundHttpError;

type Result<T> = std::result::Result<T, OutboundHttpError>;

/// Send an HTTP request and get a fully formed HTTP response.
pub fn send_request(req: Request) -> Result<Response> {
    let (req, body) = req.into_parts();

    let method = req.method.try_into()?;

    let uri = req.uri.to_string();

    let params = vec![];

    let headers = req
        .headers
        .iter()
        .map(try_header_to_strings)
        .collect::<Result<Vec<_>>>()?;

    let body = body.map(|bytes| bytes.to_vec());

    let out_req = spin_http_types::Request {
        method,
        uri,
        params,
        headers,
        body,
    };

    let spin_http_types::Response {
        status,
        headers,
        body,
    } = spin_http::send_request(&out_req)?;

    let resp_builder = http_types::response::Builder::new().status(status);
    let resp_builder = headers
        .into_iter()
        .flatten()
        .fold(resp_builder, |b, (k, v)| b.header(k, v));
    resp_builder
        .body(body.map(Into::into))
        .map_err(|_| OutboundHttpError::RuntimeError)
}

fn try_header_to_strings(
    (header_name, header_value): (&HeaderName, &HeaderValue),
) -> Result<(String, String)> {
    Ok((
        header_name.to_string(),
        header_value
            .to_str()
            .map_err(|_| OutboundHttpError::InvalidUrl)?
            .to_owned(),
    ))
}

impl TryFrom<http_types::Method> for spin_http_types::Method {
    type Error = OutboundHttpError;

    fn try_from(method: http_types::Method) -> Result<Self> {
        use http_types::Method;
        use spin_http_types::Method::*;
        Ok(match method {
            Method::GET => Get,
            Method::POST => Post,
            Method::PUT => Put,
            Method::DELETE => Delete,
            Method::PATCH => Patch,
            Method::HEAD => Head,
            Method::OPTIONS => Options,
            _ => return Err(spin_http::HttpError::RequestError),
        })
    }
}
