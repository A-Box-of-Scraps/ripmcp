use super::invalid;
use crate::error::{Error, ErrorKind};
use crate::mcp::Operation;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

pub(super) const BODY_LIMIT: usize = 256 * 1024;

pub(super) struct Network {
    client: Client,
    #[cfg(test)]
    pub loopback: bool,
}

impl Network {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .no_proxy()
                .referer(false)
                .http1_only()
                .pool_max_idle_per_host(0)
                .dns_resolver(std::sync::Arc::new(super::destination::PublicResolver))
                .build()
                .map_err(|_| invalid())?,
            #[cfg(test)]
            loopback: false,
        })
    }

    pub fn url(&self, value: &str) -> Result<Url, Error> {
        let url: Url = crate::config::schema::endpoint(value).map_err(|_| invalid())?;
        let secure = url.scheme() == "https";
        #[cfg(test)]
        let secure = secure || self.loopback && url.host_str() == Some("127.0.0.1");
        if !secure || value.len() > 8192 {
            return Err(invalid());
        }
        let allowed = match url.host() {
            Some(url::Host::Ipv4(address)) => super::destination::public(address.into()),
            Some(url::Host::Ipv6(address)) => super::destination::public(address.into()),
            Some(url::Host::Domain(host)) => host != "localhost" && !host.ends_with(".localhost"),
            None => false,
        };
        #[cfg(test)]
        let allowed = allowed || self.loopback && url.host_str() == Some("127.0.0.1");
        if !allowed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "OAuth requires public HTTPS destinations; private-network providers are not supported",
            ));
        }
        Ok(url)
    }

    pub fn get(&self, url: &Url) -> Result<RequestBuilder, Error> {
        self.url(url.as_str())?;
        Ok(self
            .client
            .get(url.clone())
            .header("accept", "application/json"))
    }

    pub fn post(&self, url: &Url, fields: &[(&str, &str)]) -> Result<RequestBuilder, Error> {
        self.url(url.as_str())?;
        let mut form: url::form_urlencoded::Serializer<'_, String> =
            url::form_urlencoded::Serializer::new(String::new());
        form.extend_pairs(fields.iter().copied());
        Ok(self
            .client
            .post(url.clone())
            .header("content-type", "application/x-www-form-urlencoded")
            .header("accept", "application/json")
            .body(form.finish()))
    }

    pub fn json(&self, url: &Url, value: &Value) -> Result<RequestBuilder, Error> {
        self.url(url.as_str())?;
        Ok(self
            .client
            .post(url.clone())
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(serde_json::to_vec(value).map_err(|_| invalid())?))
    }
}

pub(super) async fn send(
    request: RequestBuilder,
    operation: &Operation,
) -> Result<Response, Error> {
    operation
        .run(async {
            let response: Response = request.send().await.map_err(|_| {
                Error::new(
                    ErrorKind::Connection,
                    "OAuth connection failed; no automatic retry was attempted",
                )
            })?;
            if response.status().is_redirection() {
                return Err(Error::new(
                    ErrorKind::Authentication,
                    "OAuth redirect refused",
                ));
            }
            Ok(response)
        })
        .await
}

pub(super) async fn document<T: DeserializeOwned>(
    response: Response,
    operation: &Operation,
) -> Result<T, Error> {
    operation
        .run(async {
            if response
                .headers()
                .get("content-type")
                .and_then(|header| header.to_str().ok())
                .and_then(|header| header.split(';').next())
                .is_none_or(|media| !media.trim().eq_ignore_ascii_case("application/json"))
            {
                return Err(invalid());
            }
            let bytes: Vec<u8> = body(response).await?;
            let value: Value = crate::json::parse(&bytes).map_err(|_| invalid())?;
            serde_json::from_value(value).map_err(|_| invalid())
        })
        .await
}

async fn body(mut response: Response) -> Result<Vec<u8>, Error> {
    if response
        .content_length()
        .is_some_and(|size| size > BODY_LIMIT as u64)
    {
        return Err(invalid());
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if chunk.len() > BODY_LIMIT.saturating_sub(bytes.len()) {
            return Err(invalid());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn absent(status: StatusCode) -> bool {
    status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED
}
