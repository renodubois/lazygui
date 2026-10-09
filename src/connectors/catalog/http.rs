//! Read-only local example: GET /records?q=... -> {"items":[...]}.
use super::{Adapter, Error, Record, Request};
use reqwest::{Client, Url};
use serde::Deserialize;
use std::{collections::HashSet, net::IpAddr, time::Duration};

pub(super) struct Http {
    client: Client,
    endpoint: Url,
}
impl Http {
    pub(super) fn new(value: &str) -> Result<Self, Error> {
        let endpoint = Url::parse(value).map_err(|_| Error::InvalidEndpoint)?;
        let loopback = endpoint
            .host_str()
            .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if endpoint.scheme() != "http"
            || !loopback
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/records"
        {
            return Err(Error::InvalidEndpoint);
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self { client, endpoint })
    }
}
#[derive(Deserialize)]
struct Collection {
    items: Vec<Record>,
}
impl Adapter for Http {
    fn list(&self, query: String) -> Request {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        Box::pin(async move {
            let mut response = client
                .get(endpoint)
                .query(&[("q", query)])
                .send()
                .await
                .map_err(transport_error)?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(Error::Http(response.status().as_u16()));
            }
            const MAX_BODY: usize = 1024 * 1024;
            if response
                .content_length()
                .is_some_and(|size| size > MAX_BODY as u64)
            {
                return Err(Error::InvalidResponse);
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
                if body.len().saturating_add(chunk.len()) > MAX_BODY {
                    return Err(Error::InvalidResponse);
                }
                body.extend_from_slice(&chunk);
            }
            let collection: Collection =
                serde_json::from_slice(&body).map_err(|_| Error::InvalidResponse)?;
            let mut ids = HashSet::new();
            if collection.items.iter().any(|record| {
                record.id.is_empty() || record.title.is_empty() || !ids.insert(record.id.clone())
            }) {
                return Err(Error::InvalidResponse);
            }
            Ok(collection.items)
        })
    }
}
fn transport_error(error: reqwest::Error) -> Error {
    // Never expose reqwest errors/URLs directly to UI or logs.
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Transport
    }
}
#[cfg(test)]
#[path = "tests/http.rs"]
mod tests;
