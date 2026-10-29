use std::time::Duration;

use futures::{future::BoxFuture, io::AsyncReadExt as _};
use gpui_kit::http_client::{
    self, AsyncBody, CustomHeaders, HttpClient, RedirectPolicy, RequestTimeout,
};

const USER_AGENT: &str = concat!("OrbMC/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub struct ReqwestHttpClient {
    follow_redirects: reqwest::blocking::Client,
    no_redirects: reqwest::blocking::Client,
    user_agent: http_client::http::HeaderValue,
}

impl ReqwestHttpClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let user_agent = http_client::http::HeaderValue::from_static(USER_AGENT);
        let follow_redirects = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;
        let no_redirects = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        Ok(Self {
            follow_redirects,
            no_redirects,
            user_agent,
        })
    }

    fn send_blocking(
        &self,
        parts: http_client::http::request::Parts,
        request_body: Vec<u8>,
    ) -> http_client::Result<http_client::Response<AsyncBody>> {
        let redirect_policy = parts
            .extensions
            .get::<RedirectPolicy>()
            .cloned()
            .unwrap_or_default();
        let timeout = parts
            .extensions
            .get::<RequestTimeout>()
            .map(|timeout| timeout.0);
        let extra_headers = parts.extensions.get::<CustomHeaders>().cloned();

        let client = match redirect_policy {
            RedirectPolicy::NoFollow => self.no_redirects.clone(),
            RedirectPolicy::FollowLimit(limit) => reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::limited(limit as usize))
                .build()?,
            RedirectPolicy::FollowAll => self.follow_redirects.clone(),
        };

        let mut headers = parts.headers;
        if let Some(extra_headers) = extra_headers {
            for (name, value) in extra_headers.iter() {
                headers.append(name.clone(), value.clone());
            }
        }

        let mut builder = client
            .request(parts.method, parts.uri.to_string())
            .headers(headers)
            .body(request_body);
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }

        let response = builder.send()?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes()?.to_vec();

        let mut response_builder = http_client::Response::builder().status(status);
        for (name, value) in &headers {
            response_builder = response_builder.header(name, value);
        }

        Ok(response_builder.body(AsyncBody::from(body))?)
    }
}

impl HttpClient for ReqwestHttpClient {
    fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&http_client::Url> {
        None
    }

    fn send(
        &self,
        request: http_client::Request<AsyncBody>,
    ) -> BoxFuture<'static, http_client::Result<http_client::Response<AsyncBody>>> {
        let client = self.clone();
        Box::pin(async move {
            let (parts, mut body) = request.into_parts();
            let mut request_body = Vec::new();
            body.read_to_end(&mut request_body).await?;

            let (sender, receiver) = futures::channel::oneshot::channel();
            std::thread::Builder::new()
                .name("launcher-http".to_string())
                .spawn(move || {
                    let _ = sender.send(client.send_blocking(parts, request_body));
                })?;

            receiver
                .await
                .map_err(|_| http_client::anyhow!("HTTP worker ended without a response"))?
        })
    }
}
