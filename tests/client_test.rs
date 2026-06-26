//! Unit tests that inject a stub transport returning canned `(status, headers,
//! body)` triples from the fixtures in INTERFACE.md section 5. No test touches
//! the network.

use async_trait::async_trait;
use demografix::{Demografix, Error, RawResponse, Request, Transport};
use std::collections::HashMap;
use std::sync::Mutex;

/// A transport that records the request it received and returns a canned
/// response. `expect_call` is set to false when a call must never be made.
struct StubTransport {
    status: u16,
    body: String,
    expect_call: bool,
    captured: Mutex<Option<Request>>,
}

impl StubTransport {
    fn ok(body: &str) -> Self {
        StubTransport {
            status: 200,
            body: body.to_string(),
            expect_call: true,
            captured: Mutex::new(None),
        }
    }

    fn error(status: u16, body: &str) -> Self {
        StubTransport {
            status,
            body: body.to_string(),
            expect_call: true,
            captured: Mutex::new(None),
        }
    }

    /// A transport that fails the test if it is ever called.
    fn never() -> Self {
        StubTransport {
            status: 200,
            body: String::new(),
            expect_call: false,
            captured: Mutex::new(None),
        }
    }

    fn captured(&self) -> Request {
        self.captured
            .lock()
            .unwrap()
            .clone()
            .expect("transport was called")
    }
}

/// The fixture rate-limit headers, present on every response.
fn fixture_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("x-rate-limit-limit".to_string(), "25000".to_string());
    headers.insert("x-rate-limit-remaining".to_string(), "24987".to_string());
    headers.insert("x-rate-limit-reset".to_string(), "1314000".to_string());
    headers
}

#[async_trait]
impl Transport for StubTransport {
    async fn execute(&self, request: Request) -> Result<RawResponse, Error> {
        assert!(
            self.expect_call,
            "transport was called but the test expected no HTTP call"
        );
        *self.captured.lock().unwrap() = Some(request);
        Ok(RawResponse {
            status: self.status,
            headers: fixture_headers(),
            body: self.body.clone(),
        })
    }
}

// 1. Single genderize/agify/nationalize — fields parse, quota.remaining == 24987.

#[tokio::test]
async fn genderize_single_parses_with_quota() {
    let stub = StubTransport::ok(
        r#"{ "count": 1352696, "name": "peter", "gender": "male", "probability": 1.0 }"#,
    );
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.genderize("peter", None).await.unwrap();

    // Prediction fields read directly off the result through Deref.
    assert_eq!(result.name, "peter");
    assert_eq!(result.gender.as_deref(), Some("male"));
    assert_eq!(result.probability, 1.0);
    assert_eq!(result.count, 1352696);
    assert_eq!(result.country_id, None);
    // The explicit `prediction` member exposes the same values.
    assert_eq!(result.prediction.gender.as_deref(), Some("male"));
    assert_eq!(result.quota.limit, 25000);
    assert_eq!(result.quota.remaining, 24987);
    assert_eq!(result.quota.reset, 1314000);
}

#[tokio::test]
async fn agify_single_parses_with_quota() {
    let stub = StubTransport::ok(r#"{ "count": 311558, "name": "michael", "age": 57 }"#);
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.agify("michael", None).await.unwrap();

    // Prediction fields read directly off the result through Deref.
    assert_eq!(result.name, "michael");
    assert_eq!(result.age, Some(57));
    assert_eq!(result.count, 311558);
    assert_eq!(result.quota.remaining, 24987);
}

#[tokio::test]
async fn nationalize_single_parses_with_quota() {
    let stub = StubTransport::ok(
        r#"{ "count": 100783, "name": "nguyen",
            "country": [ { "country_id": "VN", "probability": 0.891132 },
                         { "country_id": "MO", "probability": 0.019031 } ] }"#,
    );
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.nationalize("nguyen").await.unwrap();

    // Prediction fields read directly off the result through Deref.
    assert_eq!(result.name, "nguyen");
    assert_eq!(result.country.len(), 2);
    assert_eq!(result.country[0].country_id, "VN");
    assert_eq!(result.country[0].probability, 0.891132);
    assert_eq!(result.count, 100783);
    assert_eq!(result.quota.remaining, 24987);
}

// 2. A batch — results length and order match input, quota parses.

#[tokio::test]
async fn agify_batch_parses_in_order_with_quota() {
    let stub = StubTransport::ok(
        r#"[ { "count": 311558, "name": "michael", "age": 57 },
            { "count": 55682,  "name": "matthew", "age": 48 } ]"#,
    );
    let client = Demografix::with_transport(stub, "test-key");
    let batch = client
        .agify_batch(&["michael", "matthew"], None)
        .await
        .unwrap();

    assert_eq!(batch.results.len(), 2);
    assert_eq!(batch.results[0].name, "michael");
    assert_eq!(batch.results[0].age, Some(57));
    assert_eq!(batch.results[1].name, "matthew");
    assert_eq!(batch.results[1].age, Some(48));
    assert_eq!(batch.quota.remaining, 24987);
}

#[tokio::test]
async fn batch_builds_repeated_name_params() {
    let stub = StubTransport::ok(r#"[]"#);
    // Capture via a separate constructed stub we keep a reference to.
    let captured = run_and_capture(stub, |client| async move {
        let _ = client.agify_batch(&["a", "b", "c"], None).await;
    })
    .await;

    let names: Vec<&str> = captured
        .query
        .iter()
        .filter(|(k, _)| k == "name[]")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    // No single `name` key in a batch.
    assert!(captured.query.iter().all(|(k, _)| k != "name"));
    // The API key is always sent on the wire.
    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "apikey" && v == "test-key"));
}

// 3. Null prediction — gender/age null / country empty, no error raised.

#[tokio::test]
async fn genderize_null_prediction_is_success() {
    let stub =
        StubTransport::ok(r#"{ "name": "xÿz", "gender": null, "probability": 0.0, "count": 0 }"#);
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.genderize("xÿz", None).await.unwrap();

    assert_eq!(result.gender, None);
    assert_eq!(result.probability, 0.0);
    assert_eq!(result.count, 0);
}

#[tokio::test]
async fn agify_null_prediction_is_success() {
    let stub = StubTransport::ok(r#"{ "name": "xÿz", "age": null, "count": 0 }"#);
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.agify("xÿz", None).await.unwrap();

    assert_eq!(result.age, None);
    assert_eq!(result.count, 0);
}

#[tokio::test]
async fn nationalize_null_prediction_is_success() {
    let stub = StubTransport::ok(r#"{ "name": "xÿz", "country": [], "count": 0 }"#);
    let client = Demografix::with_transport(stub, "test-key");
    let result = client.nationalize("xÿz").await.unwrap();

    assert!(result.country.is_empty());
    assert_eq!(result.count, 0);
}

// 4. country_id round-trips into the request and parses back from the response.

#[tokio::test]
async fn country_id_round_trips() {
    let stub = StubTransport::ok(
        r#"{ "count": 196601, "name": "kim", "gender": "female", "country_id": "US", "probability": 0.94 }"#,
    );
    let captured = run_and_capture(stub, |client| async move {
        let result = client.genderize("kim", Some("US")).await.unwrap();
        // Explicit member access and the Deref shortcut agree.
        assert_eq!(result.prediction.country_id.as_deref(), Some("US"));
        assert_eq!(result.country_id.as_deref(), Some("US"));
        assert_eq!(result.gender.as_deref(), Some("female"));
    })
    .await;

    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "country_id" && v == "US"));
    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "name" && v == "kim"));
    // The API key is always sent on the wire.
    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "apikey" && v == "test-key"));
}

// 5. Batch of 11 names raises ValidationError with no HTTP call.

#[tokio::test]
async fn batch_over_ten_raises_validation_without_http() {
    let stub = StubTransport::never();
    let client = Demografix::with_transport(stub, "test-key");
    let names: Vec<&str> = vec!["n"; 11];
    let err = client.genderize_batch(&names, None).await.unwrap_err();

    match &err {
        Error::Validation { status, quota, .. } => {
            assert_eq!(*status, 0);
            assert!(quota.is_none());
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
    assert_eq!(err.status(), None);
}

// 7. Constructing without a usable api_key raises ValidationError with no HTTP
//    call. An empty or blank key is rejected before the transport is touched.

#[tokio::test]
async fn empty_api_key_raises_validation_without_http() {
    let stub = StubTransport::never();
    let client = Demografix::with_transport(stub, "");
    let err = client.genderize("peter", None).await.unwrap_err();

    match &err {
        Error::Validation {
            status,
            message,
            quota,
        } => {
            assert_eq!(*status, 0);
            assert_eq!(message, "api_key is required");
            assert!(quota.is_none());
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
    assert_eq!(err.status(), None);
}

#[tokio::test]
async fn blank_api_key_raises_validation_without_http() {
    let stub = StubTransport::never();
    let client = Demografix::with_transport(stub, "   ");
    let err = client.nationalize("peter").await.unwrap_err();

    match &err {
        Error::Validation { message, .. } => assert_eq!(message, "api_key is required"),
        other => panic!("expected ValidationError, got {other:?}"),
    }
}

// 6. 401/402/422/429 map to the right error types carrying status, message,
//    and (429) quota.

#[tokio::test]
async fn status_401_maps_to_auth() {
    let stub = StubTransport::error(401, r#"{ "error": "Invalid API key" }"#);
    let client = Demografix::with_transport(stub, "bad");
    let err = client.genderize("peter", None).await.unwrap_err();

    match err {
        Error::Auth {
            status,
            message,
            quota,
        } => {
            assert_eq!(status, 401);
            assert_eq!(message, "Invalid API key");
            assert_eq!(quota.unwrap().remaining, 24987);
        }
        other => panic!("expected AuthError, got {other:?}"),
    }
}

#[tokio::test]
async fn status_402_maps_to_subscription() {
    let stub = StubTransport::error(402, r#"{ "error": "Subscription is not active" }"#);
    let client = Demografix::with_transport(stub, "key");
    let err = client.agify("michael", None).await.unwrap_err();

    match err {
        Error::Subscription {
            status, message, ..
        } => {
            assert_eq!(status, 402);
            assert_eq!(message, "Subscription is not active");
        }
        other => panic!("expected SubscriptionError, got {other:?}"),
    }
}

#[tokio::test]
async fn status_422_maps_to_validation() {
    let stub = StubTransport::error(422, r#"{ "error": "Missing 'name' parameter" }"#);
    let client = Demografix::with_transport(stub, "test-key");
    let err = client.nationalize("").await.unwrap_err();

    match err {
        Error::Validation {
            status, message, ..
        } => {
            assert_eq!(status, 422);
            assert_eq!(message, "Missing 'name' parameter");
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
}

#[tokio::test]
async fn status_429_maps_to_rate_limit_with_quota() {
    let stub = StubTransport::error(429, r#"{ "error": "Request limit reached" }"#);
    let client = Demografix::with_transport(stub, "key");
    let err = client.genderize("peter", None).await.unwrap_err();

    match err {
        Error::RateLimit {
            status,
            message,
            quota,
        } => {
            assert_eq!(status, 429);
            assert_eq!(message, "Request limit reached");
            assert_eq!(quota.remaining, 24987);
            assert_eq!(quota.reset, 1314000);
        }
        other => panic!("expected RateLimitError, got {other:?}"),
    }
}

// A non-JSON error body maps to Transport, not a status-typed error. The body
// is parsed as JSON first, regardless of status, so an HTML 502 from a proxy
// becomes a transport failure carrying the status and the parsed quota.

#[tokio::test]
async fn non_json_error_body_maps_to_transport() {
    let stub = StubTransport::error(502, "<html>502 Bad Gateway</html>");
    let client = Demografix::with_transport(stub, "test-key");
    let err = client.genderize("peter", None).await.unwrap_err();

    match err {
        Error::Transport { status, quota, .. } => {
            assert_eq!(status, Some(502));
            assert_eq!(quota.unwrap().remaining, 24987);
        }
        other => panic!("expected TransportError, got {other:?}"),
    }
}

// Helpers.

/// Run a closure against a client built over `stub`, then return the request the
/// stub captured. The stub is owned here so we can read it after the call.
async fn run_and_capture<F, Fut>(stub: StubTransport, body: F) -> Request
where
    F: FnOnce(Demografix<StubTransportHandle>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    use std::sync::Arc;
    let shared = Arc::new(stub);
    let handle = StubTransportHandle {
        inner: Arc::clone(&shared),
    };
    let client = Demografix::with_transport(handle, "test-key");
    body(client).await;
    shared.captured()
}

/// A cloneable handle that delegates to a shared [`StubTransport`], so the test
/// can inspect the captured request after the client has used the transport.
struct StubTransportHandle {
    inner: std::sync::Arc<StubTransport>,
}

#[async_trait]
impl Transport for StubTransportHandle {
    async fn execute(&self, request: Request) -> Result<RawResponse, Error> {
        self.inner.execute(request).await
    }
}
