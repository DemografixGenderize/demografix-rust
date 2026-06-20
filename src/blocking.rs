//! The synchronous Demografix client, behind the `blocking` feature.
//!
//! [`BlockingDemografix`] mirrors the async [`crate::Demografix`] surface but
//! returns its results without `.await`. It is generic over a
//! [`BlockingTransport`]; the public constructor wires in a reqwest blocking
//! transport, and tests inject a stub. The base URLs and the User-Agent are the
//! same hardcoded constants used by the async client.

use crate::client::{
    build_query, decode_response, validate_batch_size, Request, AGIFY_BASE, GENDERIZE_BASE,
    NATIONALIZE_BASE, USER_AGENT,
};
use crate::errors::Error;
use crate::models::{
    AgifyPrediction, AgifyResult, Batch, GenderizePrediction, GenderizeResult,
    NationalizePrediction, NationalizeResult, Quota,
};
use crate::RawResponse;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Default request timeout, matching the async client.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The synchronous seam that performs HTTP. The real implementation uses
/// reqwest's blocking client; tests supply a stub.
pub trait BlockingTransport: Send + Sync {
    /// Execute the request and return the raw response, or a transport-level
    /// failure (network error, timeout).
    fn execute(&self, request: Request) -> Result<RawResponse, Error>;
}

/// The reqwest-backed blocking transport used in production.
pub struct ReqwestBlockingTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestBlockingTransport {
    fn new(timeout: Duration) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest blocking client builds with a timeout");
        ReqwestBlockingTransport { client }
    }
}

impl BlockingTransport for ReqwestBlockingTransport {
    fn execute(&self, request: Request) -> Result<RawResponse, Error> {
        let response = self
            .client
            .get(&request.url)
            .query(&request.query)
            .header(reqwest::header::USER_AGENT, &request.user_agent)
            .send()
            .map_err(|err| Error::Transport {
                message: err.to_string(),
                status: err.status().map(|s| s.as_u16()),
                quota: None,
            })?;

        let status = response.status().as_u16();
        let mut headers = std::collections::HashMap::new();
        for (name, value) in response.headers().iter() {
            if let Ok(value) = value.to_str() {
                headers.insert(name.as_str().to_string(), value.to_string());
            }
        }

        let body = response.text().map_err(|err| Error::Transport {
            message: err.to_string(),
            status: Some(status),
            quota: None,
        })?;

        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}

/// The synchronous Demografix client.
///
/// Construct one with [`BlockingDemografix::new`] (or
/// [`BlockingDemografix::with_timeout`]) and call the per-service methods. The
/// client holds the API key and the timeout; it never holds or caches quota.
pub struct BlockingDemografix<T: BlockingTransport = ReqwestBlockingTransport> {
    transport: T,
    api_key: Option<String>,
}

impl BlockingDemografix<ReqwestBlockingTransport> {
    /// Build a client with the default 10-second timeout.
    ///
    /// Pass `Some(key)` to authenticate, or `None` for the free per-IP tier.
    pub fn new(api_key: Option<&str>) -> Self {
        Self::with_timeout(api_key, DEFAULT_TIMEOUT)
    }

    /// Build a client with a custom request timeout.
    pub fn with_timeout(api_key: Option<&str>, timeout: Duration) -> Self {
        BlockingDemografix {
            transport: ReqwestBlockingTransport::new(timeout),
            api_key: api_key.map(str::to_string),
        }
    }
}

impl<T: BlockingTransport> BlockingDemografix<T> {
    /// Build a client over a custom transport. Internal; used by tests to inject
    /// a stub. The public API does not expose a base-URL option.
    #[doc(hidden)]
    pub fn with_transport(transport: T, api_key: Option<&str>) -> Self {
        BlockingDemografix {
            transport,
            api_key: api_key.map(str::to_string),
        }
    }

    /// Predict gender for one name.
    pub fn genderize(
        &self,
        name: &str,
        country_id: Option<&str>,
    ) -> Result<GenderizeResult, Error> {
        let request = self.build_request(GENDERIZE_BASE, &[name], country_id);
        let (prediction, quota) = self.send_single(request)?;
        Ok(GenderizeResult { prediction, quota })
    }

    /// Predict gender for a list of names (maximum 10).
    pub fn genderize_batch(
        &self,
        names: &[&str],
        country_id: Option<&str>,
    ) -> Result<Batch<GenderizePrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(GENDERIZE_BASE, names, country_id);
        let (results, quota) = self.send_batch(request)?;
        Ok(Batch { results, quota })
    }

    /// Predict age for one name.
    pub fn agify(&self, name: &str, country_id: Option<&str>) -> Result<AgifyResult, Error> {
        let request = self.build_request(AGIFY_BASE, &[name], country_id);
        let (prediction, quota) = self.send_single(request)?;
        Ok(AgifyResult { prediction, quota })
    }

    /// Predict age for a list of names (maximum 10).
    pub fn agify_batch(
        &self,
        names: &[&str],
        country_id: Option<&str>,
    ) -> Result<Batch<AgifyPrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(AGIFY_BASE, names, country_id);
        let (results, quota) = self.send_batch(request)?;
        Ok(Batch { results, quota })
    }

    /// Predict nationality for one name. Nationalize takes no `country_id`.
    pub fn nationalize(&self, name: &str) -> Result<NationalizeResult, Error> {
        let request = self.build_request(NATIONALIZE_BASE, &[name], None);
        let (prediction, quota) = self.send_single(request)?;
        Ok(NationalizeResult { prediction, quota })
    }

    /// Predict nationality for a list of names (maximum 10).
    pub fn nationalize_batch(
        &self,
        names: &[&str],
    ) -> Result<Batch<NationalizePrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(NATIONALIZE_BASE, names, None);
        let (results, quota) = self.send_batch(request)?;
        Ok(Batch { results, quota })
    }

    fn build_request(&self, base: &str, names: &[&str], country_id: Option<&str>) -> Request {
        Request {
            url: base.to_string(),
            query: build_query(names, country_id, self.api_key.as_deref()),
            user_agent: USER_AGENT.to_string(),
        }
    }

    fn send_single<P: DeserializeOwned>(&self, request: Request) -> Result<(P, Quota), Error> {
        let response = self.transport.execute(request)?;
        decode_response(&response)
    }

    fn send_batch<P: DeserializeOwned>(&self, request: Request) -> Result<(Vec<P>, Quota), Error> {
        let response = self.transport.execute(request)?;
        let (results, quota) = decode_response::<Vec<P>>(&response)?;
        Ok((results, quota))
    }
}
