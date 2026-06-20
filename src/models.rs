//! Response models for the Demografix APIs.
//!
//! Prediction structs deserialize directly from the API JSON bodies. Each
//! single-call result pairs one prediction with the response [`Quota`] and
//! derefs to that prediction, so prediction fields are reachable directly on the
//! result (`result.gender`, not `result.prediction.gender`) while `result.quota`
//! still reads the quota. Batch responses hold a vector of predictions alongside
//! one quota.

use serde::Deserialize;
use std::ops::Deref;

/// Remaining request budget, read from the rate-limit response headers.
///
/// Quota is read off a returned value or a raised error. It is never cached on
/// the client.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Quota {
    /// Names allowed in the current window.
    pub limit: i64,
    /// Names left in the current window. The server clamps this to 0.
    pub remaining: i64,
    /// Seconds until the window resets.
    pub reset: i64,
}

/// A single gender prediction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct GenderizePrediction {
    /// The input name, echoed back.
    pub name: String,
    /// `"male"`, `"female"`, or `None` when there is no match.
    pub gender: Option<String>,
    /// Probability between 0 and 1, rounded to 2 decimals.
    pub probability: f64,
    /// Source rows behind the prediction.
    pub count: i64,
    /// The country scope, uppercase, present only when the request sent one.
    #[serde(default)]
    pub country_id: Option<String>,
}

/// A single age prediction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct AgifyPrediction {
    /// The input name, echoed back.
    pub name: String,
    /// Predicted age, or `None` when there is no match.
    pub age: Option<i64>,
    /// Source rows behind the prediction.
    pub count: i64,
    /// The country scope, uppercase, present only when the request sent one.
    #[serde(default)]
    pub country_id: Option<String>,
}

/// One country candidate within a nationality prediction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct NationalizeCountry {
    /// ISO 3166-1 alpha-2 country code.
    pub country_id: String,
    /// Probability between 0 and 1, rounded to 6 decimals.
    pub probability: f64,
}

/// A single nationality prediction.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct NationalizePrediction {
    /// The input name, echoed back.
    pub name: String,
    /// Up to 5 candidates, descending probability. Empty when there is no match.
    pub country: Vec<NationalizeCountry>,
    /// Source rows behind the prediction.
    pub count: i64,
}

/// A gender prediction plus the quota for the response.
///
/// Derefs to [`GenderizePrediction`], so the prediction fields read directly off
/// the result: `result.gender`, `result.probability`, `result.count`. The
/// prediction is also available explicitly as `result.prediction`, and the quota
/// as `result.quota`.
#[derive(Debug, Clone, PartialEq)]
pub struct GenderizeResult {
    /// The prediction fields. Also reachable directly through `Deref`.
    pub prediction: GenderizePrediction,
    /// Remaining request budget after this call.
    pub quota: Quota,
}

impl Deref for GenderizeResult {
    type Target = GenderizePrediction;

    fn deref(&self) -> &Self::Target {
        &self.prediction
    }
}

/// An age prediction plus the quota for the response.
///
/// Derefs to [`AgifyPrediction`], so the prediction fields read directly off the
/// result: `result.age`, `result.count`. The prediction is also available
/// explicitly as `result.prediction`, and the quota as `result.quota`.
#[derive(Debug, Clone, PartialEq)]
pub struct AgifyResult {
    /// The prediction fields. Also reachable directly through `Deref`.
    pub prediction: AgifyPrediction,
    /// Remaining request budget after this call.
    pub quota: Quota,
}

impl Deref for AgifyResult {
    type Target = AgifyPrediction;

    fn deref(&self) -> &Self::Target {
        &self.prediction
    }
}

/// A nationality prediction plus the quota for the response.
///
/// Derefs to [`NationalizePrediction`], so the prediction fields read directly
/// off the result: `result.country`, `result.count`. The prediction is also
/// available explicitly as `result.prediction`, and the quota as `result.quota`.
#[derive(Debug, Clone, PartialEq)]
pub struct NationalizeResult {
    /// The prediction fields. Also reachable directly through `Deref`.
    pub prediction: NationalizePrediction,
    /// Remaining request budget after this call.
    pub quota: Quota,
}

impl Deref for NationalizeResult {
    type Target = NationalizePrediction;

    fn deref(&self) -> &Self::Target {
        &self.prediction
    }
}

/// A batch response: the per-name predictions in input order, plus one quota for
/// the whole response.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch<T> {
    /// Predictions in input order, without their own quota.
    pub results: Vec<T>,
    /// Remaining request budget after this call.
    pub quota: Quota,
}
