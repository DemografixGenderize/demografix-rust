//! Unit tests for the synchronous client behind the `blocking` feature. They
//! inject a stub transport returning canned `(status, headers, body)` triples
//! from the fixtures in INTERFACE.md section 5. No test touches the network.
//!
//! The whole file compiles only when the `blocking` feature is on; run with
//! `cargo test --features blocking`.

#![cfg(feature = "blocking")]

use demografix::{BlockingDemografix, BlockingTransport, Error, RawResponse, Request};
use std::collections::HashMap;
use std::sync::Mutex;

/// A transport that records the request it received and returns a canned
/// response. `expect_call` is false when a call must never be made.
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

impl BlockingTransport for StubTransport {
    fn execute(&self, request: Request) -> Result<RawResponse, Error> {
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

// 1. Single — fields parse, quota.remaining == 24987.

#[test]
fn genderize_single_parses_with_quota() {
    let stub = StubTransport::ok(
        r#"{ "count": 1352696, "name": "peter", "gender": "male", "probability": 1.0 }"#,
    );
    let client = BlockingDemografix::with_transport(stub, None);
    let result = client.genderize("peter", None).unwrap();

    // Prediction fields read directly off the result through Deref.
    assert_eq!(result.name, "peter");
    assert_eq!(result.gender.as_deref(), Some("male"));
    assert_eq!(result.probability, 1.0);
    assert_eq!(result.count, 1352696);
    assert_eq!(result.country_id, None);
    // The explicit `prediction` member exposes the same values.
    assert_eq!(result.prediction.gender.as_deref(), Some("male"));
    assert_eq!(result.quota.remaining, 24987);
}

#[test]
fn nationalize_single_parses_with_quota() {
    let stub = StubTransport::ok(
        r#"{ "count": 100783, "name": "nguyen",
            "country": [ { "country_id": "VN", "probability": 0.891132 },
                         { "country_id": "MO", "probability": 0.019031 } ] }"#,
    );
    let client = BlockingDemografix::with_transport(stub, None);
    let result = client.nationalize("nguyen").unwrap();

    // Prediction fields read directly off the result through Deref.
    assert_eq!(result.country.len(), 2);
    assert_eq!(result.country[0].country_id, "VN");
    assert_eq!(result.quota.remaining, 24987);
}

// 2. A batch — results length and order match input, quota parses.

#[test]
fn agify_batch_parses_in_order_with_quota() {
    let stub = StubTransport::ok(
        r#"[ { "count": 311558, "name": "michael", "age": 57 },
            { "count": 55682,  "name": "matthew", "age": 48 } ]"#,
    );
    let client = BlockingDemografix::with_transport(stub, None);
    let batch = client.agify_batch(&["michael", "matthew"], None).unwrap();

    assert_eq!(batch.results.len(), 2);
    assert_eq!(batch.results[0].name, "michael");
    assert_eq!(batch.results[0].age, Some(57));
    assert_eq!(batch.results[1].name, "matthew");
    assert_eq!(batch.results[1].age, Some(48));
    assert_eq!(batch.quota.remaining, 24987);
}

// 3. Null prediction — age null, no error raised.

#[test]
fn agify_null_prediction_is_success() {
    let stub = StubTransport::ok(r#"{ "name": "xÿz", "age": null, "count": 0 }"#);
    let client = BlockingDemografix::with_transport(stub, None);
    let result = client.agify("xÿz", None).unwrap();

    assert_eq!(result.age, None);
    assert_eq!(result.count, 0);
}

// 4. country_id round-trips into the request and parses back from the response.

#[test]
fn country_id_round_trips() {
    let stub = StubTransport::ok(
        r#"{ "count": 196601, "name": "kim", "gender": "female", "country_id": "US", "probability": 0.94 }"#,
    );
    let shared = std::sync::Arc::new(stub);
    let client =
        BlockingDemografix::with_transport(SharedStub(std::sync::Arc::clone(&shared)), None);
    let result = client.genderize("kim", Some("US")).unwrap();
    // Explicit member access and the Deref shortcut agree.
    assert_eq!(result.prediction.country_id.as_deref(), Some("US"));
    assert_eq!(result.country_id.as_deref(), Some("US"));

    let captured = shared.captured();
    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "country_id" && v == "US"));
    assert!(captured
        .query
        .iter()
        .any(|(k, v)| k == "name" && v == "kim"));
}

/// A cloneable handle delegating to a shared stub, so a test can inspect the
/// captured request after the client has used the transport.
struct SharedStub(std::sync::Arc<StubTransport>);

impl BlockingTransport for SharedStub {
    fn execute(&self, request: Request) -> Result<RawResponse, Error> {
        self.0.execute(request)
    }
}

// 5. Batch of 11 names raises ValidationError with no HTTP call.

#[test]
fn batch_over_ten_raises_validation_without_http() {
    let stub = StubTransport::never();
    let client = BlockingDemografix::with_transport(stub, None);
    let names: Vec<&str> = vec!["n"; 11];
    let err = client.genderize_batch(&names, None).unwrap_err();

    match &err {
        Error::Validation { status, quota, .. } => {
            assert_eq!(*status, 0);
            assert!(quota.is_none());
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
    assert_eq!(err.status(), None);
}

// 6. 401/402/422/429 map to the right error types carrying status, message,
//    and (429) quota.

#[test]
fn status_401_maps_to_auth() {
    let stub = StubTransport::error(401, r#"{ "error": "Invalid API key" }"#);
    let client = BlockingDemografix::with_transport(stub, Some("bad"));
    let err = client.genderize("peter", None).unwrap_err();

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

#[test]
fn status_402_maps_to_subscription() {
    let stub = StubTransport::error(402, r#"{ "error": "Subscription is not active" }"#);
    let client = BlockingDemografix::with_transport(stub, Some("key"));
    let err = client.agify("michael", None).unwrap_err();

    match err {
        Error::Subscription { status, .. } => assert_eq!(status, 402),
        other => panic!("expected SubscriptionError, got {other:?}"),
    }
}

#[test]
fn status_422_maps_to_validation() {
    let stub = StubTransport::error(422, r#"{ "error": "Missing 'name' parameter" }"#);
    let client = BlockingDemografix::with_transport(stub, None);
    let err = client.nationalize("").unwrap_err();

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

#[test]
fn status_429_maps_to_rate_limit_with_quota() {
    let stub = StubTransport::error(429, r#"{ "error": "Request limit reached" }"#);
    let client = BlockingDemografix::with_transport(stub, Some("key"));
    let err = client.genderize("peter", None).unwrap_err();

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

#[test]
fn non_json_error_body_maps_to_transport() {
    let stub = StubTransport::error(502, "<html>502 Bad Gateway</html>");
    let client = BlockingDemografix::with_transport(stub, None);
    let err = client.genderize("peter", None).unwrap_err();

    match err {
        Error::Transport { status, quota, .. } => {
            assert_eq!(status, Some(502));
            assert_eq!(quota.unwrap().remaining, 24987);
        }
        other => panic!("expected TransportError, got {other:?}"),
    }
}
