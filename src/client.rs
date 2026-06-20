//! The async Demografix client and its internal transport seam.
//!
//! [`Demografix`] is generic over a [`Transport`]. The public constructor wires
//! in the real reqwest-backed transport; tests inject a stub that returns canned
//! `(status, headers, body)` triples without touching the network. The base URLs
//! and the User-Agent are hardcoded constants, not options.

use crate::errors::Error;
use crate::models::{
    AgifyPrediction, AgifyResult, Batch, GenderizePrediction, GenderizeResult,
    NationalizePrediction, NationalizeResult, Quota,
};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

/// Hardcoded host for genderize.
pub(crate) const GENDERIZE_BASE: &str = "https://api.genderize.io/";
/// Hardcoded host for agify.
pub(crate) const AGIFY_BASE: &str = "https://api.agify.io/";
/// Hardcoded host for nationalize.
pub(crate) const NATIONALIZE_BASE: &str = "https://api.nationalize.io/";
/// Sent on every request. Not configurable.
pub(crate) const USER_AGENT: &str = concat!("demografix-rust/", env!("CARGO_PKG_VERSION"));
/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum names per batch request; the server rejects more.
const MAX_BATCH: usize = 10;

/// A transport-agnostic outbound request.
///
/// Query parameters are an ordered list because `name[]` repeats and order is
/// part of the wire contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Absolute URL of the service host.
    pub url: String,
    /// Ordered query parameters, including repeated `name[]` keys.
    pub query: Vec<(String, String)>,
    /// The hardcoded User-Agent value.
    pub user_agent: String,
}

/// A transport-agnostic inbound response.
#[derive(Debug, Clone)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers. Keys may be any case; the client parses them
    /// case-insensitively.
    pub headers: HashMap<String, String>,
    /// Raw response body.
    pub body: String,
}

/// The internal seam that performs HTTP. The real implementation uses reqwest;
/// tests supply a stub.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Execute the request and return the raw response, or a transport-level
    /// failure (network error, timeout).
    async fn execute(&self, request: Request) -> Result<RawResponse, Error>;
}

/// The reqwest-backed transport used in production.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Build a transport with the given timeout.
    fn new(timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest client builds with a timeout");
        ReqwestTransport { client }
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn execute(&self, request: Request) -> Result<RawResponse, Error> {
        let response = self
            .client
            .get(&request.url)
            .query(&request.query)
            .header(reqwest::header::USER_AGENT, &request.user_agent)
            .send()
            .await
            .map_err(|err| Error::Transport {
                message: err.to_string(),
                status: err.status().map(|s| s.as_u16()),
                quota: None,
            })?;

        let status = response.status().as_u16();
        let mut headers = HashMap::new();
        for (name, value) in response.headers().iter() {
            if let Ok(value) = value.to_str() {
                headers.insert(name.as_str().to_string(), value.to_string());
            }
        }

        let body = response.text().await.map_err(|err| Error::Transport {
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

/// The Demografix client.
///
/// Construct one with [`Demografix::new`] (or [`Demografix::with_timeout`]) and
/// call the per-service methods. The client holds the API key and the timeout;
/// it never holds or caches quota.
pub struct Demografix<T: Transport = ReqwestTransport> {
    transport: T,
    api_key: Option<String>,
}

impl Demografix<ReqwestTransport> {
    /// Build a client with the default 10-second timeout.
    ///
    /// Pass `Some(key)` to authenticate, or `None` for the free per-IP tier.
    pub fn new(api_key: Option<&str>) -> Self {
        Self::with_timeout(api_key, DEFAULT_TIMEOUT)
    }

    /// Build a client with a custom request timeout.
    pub fn with_timeout(api_key: Option<&str>, timeout: Duration) -> Self {
        Demografix {
            transport: ReqwestTransport::new(timeout),
            api_key: api_key.map(str::to_string),
        }
    }
}

impl<T: Transport> Demografix<T> {
    /// Build a client over a custom transport. Internal; used by tests to inject
    /// a stub. The public API does not expose a base-URL option.
    #[doc(hidden)]
    pub fn with_transport(transport: T, api_key: Option<&str>) -> Self {
        Demografix {
            transport,
            api_key: api_key.map(str::to_string),
        }
    }

    /// Predict gender for one name.
    pub async fn genderize(
        &self,
        name: &str,
        country_id: Option<&str>,
    ) -> Result<GenderizeResult, Error> {
        let request = self.build_request(GENDERIZE_BASE, &[name], country_id);
        let (prediction, quota) = self.send_single(request).await?;
        Ok(GenderizeResult { prediction, quota })
    }

    /// Predict gender for a list of names (maximum 10).
    pub async fn genderize_batch(
        &self,
        names: &[&str],
        country_id: Option<&str>,
    ) -> Result<Batch<GenderizePrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(GENDERIZE_BASE, names, country_id);
        let (results, quota) = self.send_batch(request).await?;
        Ok(Batch { results, quota })
    }

    /// Predict age for one name.
    pub async fn agify(
        &self,
        name: &str,
        country_id: Option<&str>,
    ) -> Result<AgifyResult, Error> {
        let request = self.build_request(AGIFY_BASE, &[name], country_id);
        let (prediction, quota) = self.send_single(request).await?;
        Ok(AgifyResult { prediction, quota })
    }

    /// Predict age for a list of names (maximum 10).
    pub async fn agify_batch(
        &self,
        names: &[&str],
        country_id: Option<&str>,
    ) -> Result<Batch<AgifyPrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(AGIFY_BASE, names, country_id);
        let (results, quota) = self.send_batch(request).await?;
        Ok(Batch { results, quota })
    }

    /// Predict nationality for one name. Nationalize takes no `country_id`.
    pub async fn nationalize(&self, name: &str) -> Result<NationalizeResult, Error> {
        let request = self.build_request(NATIONALIZE_BASE, &[name], None);
        let (prediction, quota) = self.send_single(request).await?;
        Ok(NationalizeResult { prediction, quota })
    }

    /// Predict nationality for a list of names (maximum 10).
    pub async fn nationalize_batch(
        &self,
        names: &[&str],
    ) -> Result<Batch<NationalizePrediction>, Error> {
        validate_batch_size(names)?;
        let request = self.build_request(NATIONALIZE_BASE, names, None);
        let (results, quota) = self.send_batch(request).await?;
        Ok(Batch { results, quota })
    }

    /// Build a request, adding `name`/`name[]`, `country_id`, and `apikey` only
    /// as appropriate.
    fn build_request(&self, base: &str, names: &[&str], country_id: Option<&str>) -> Request {
        Request {
            url: base.to_string(),
            query: build_query(names, country_id, self.api_key.as_deref()),
            user_agent: USER_AGENT.to_string(),
        }
    }

    /// Send a single-name request and parse one prediction plus quota.
    async fn send_single<P: DeserializeOwned>(
        &self,
        request: Request,
    ) -> Result<(P, Quota), Error> {
        let response = self.transport.execute(request).await?;
        decode_response(&response)
    }

    /// Send a batch request and parse a vector of predictions plus quota.
    async fn send_batch<P: DeserializeOwned>(
        &self,
        request: Request,
    ) -> Result<(Vec<P>, Quota), Error> {
        let response = self.transport.execute(request).await?;
        let (results, quota) = decode_response::<Vec<P>>(&response)?;
        Ok((results, quota))
    }
}

/// Build the ordered query parameters for a request: a single `name=` for one
/// name or repeated `name[]=` for a batch, then `country_id` and `apikey` only
/// when set. Shared by the async and blocking clients.
pub(crate) fn build_query(
    names: &[&str],
    country_id: Option<&str>,
    api_key: Option<&str>,
) -> Vec<(String, String)> {
    let mut query: Vec<(String, String)> = Vec::new();
    if names.len() == 1 {
        query.push(("name".to_string(), names[0].to_string()));
    } else {
        for name in names {
            query.push(("name[]".to_string(), name.to_string()));
        }
    }
    if let Some(country_id) = country_id {
        query.push(("country_id".to_string(), country_id.to_string()));
    }
    if let Some(api_key) = api_key {
        query.push(("apikey".to_string(), api_key.to_string()));
    }
    query
}

/// Reject a batch over 10 names before any HTTP call.
pub(crate) fn validate_batch_size(names: &[&str]) -> Result<(), Error> {
    if names.len() > MAX_BATCH {
        return Err(Error::Validation {
            status: 0,
            message: format!(
                "batch of {} names exceeds the maximum of {}",
                names.len(),
                MAX_BATCH
            ),
            quota: None,
        });
    }
    Ok(())
}

/// Parse the three rate-limit headers case-insensitively into a [`Quota`].
/// Returns `None` if any header is missing or unparsable.
pub(crate) fn parse_quota(headers: &HashMap<String, String>) -> Option<Quota> {
    let lookup = |name: &str| -> Option<i64> {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .and_then(|(_, value)| value.trim().parse::<i64>().ok())
    };
    Some(Quota {
        limit: lookup("x-rate-limit-limit")?,
        remaining: lookup("x-rate-limit-remaining")?,
        reset: lookup("x-rate-limit-reset")?,
    })
}

/// Decode a raw response into a success payload plus quota, mirroring the
/// canonical Python `_decode`. Shared by the async and blocking clients so both
/// share identical transport-agnostic semantics.
///
/// The body is parsed as JSON **first**, regardless of HTTP status:
///
/// - A non-JSON or empty body maps to [`Error::Transport`], carrying the HTTP
///   status and the parsed quota (present when the rate-limit headers are).
/// - Only a well-formed JSON body branches on status: a non-2xx status produces
///   the matching typed error (auth/subscription/validation/rate-limit/api),
///   while a 2xx status deserializes into the success payload `P`.
/// - A structurally incompatible success body (valid JSON, wrong shape) maps to
///   [`Error::Transport`].
pub(crate) fn decode_response<P: DeserializeOwned>(
    response: &RawResponse,
) -> Result<(P, Quota), Error> {
    let quota = parse_quota(&response.headers);

    // Parse the body as JSON first, regardless of status. A non-JSON or empty
    // body is a transport-level failure even when the status would otherwise
    // map to a typed error (e.g. an HTML 502 from a proxy).
    let value: serde_json::Value =
        serde_json::from_str(&response.body).map_err(|err| Error::Transport {
            message: format!("response body is not valid JSON: {err}"),
            status: Some(response.status),
            quota: quota.clone(),
        })?;

    // The body is well-formed JSON. Branch on status.
    if !(200..300).contains(&response.status) {
        let message = parse_error_message(&value)
            .unwrap_or_else(|| format!("request failed with status {}", response.status));
        return Err(Error::from_status(response.status, message, quota));
    }

    // A successful response must carry quota headers per the contract; their
    // absence is a transport anomaly.
    let quota = quota.ok_or_else(|| Error::Transport {
        message: "response is missing rate-limit headers".to_string(),
        status: Some(response.status),
        quota: None,
    })?;

    // Deserialize the success payload. A structurally incompatible body (valid
    // JSON, wrong shape) maps to a transport error.
    let payload = serde_json::from_value::<P>(value).map_err(|err| Error::Transport {
        message: format!("failed to parse response body: {err}"),
        status: Some(response.status),
        quota: Some(quota.clone()),
    })?;

    Ok((payload, quota))
}

/// Extract the `error` string from an already-parsed JSON body, if present.
fn parse_error_message(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.as_str())
        .map(str::to_string)
}
