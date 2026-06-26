//! Official Rust client for the Demografix APIs: genderize.io, agify.io, and
//! nationalize.io.
//!
//! One client covers all three services through the same shape. Each call
//! returns prediction fields plus the remaining [`Quota`] read from the
//! response headers. Batch calls aggregate up to 10 names per request.
//!
//! A single-name result derefs to its prediction, so the prediction fields read
//! directly off the result (`result.gender`), while `result.quota` reads the
//! quota.
//!
//! ```no_run
//! use demografix::Demografix;
//!
//! # async fn run() -> Result<(), demografix::Error> {
//! let client = Demografix::new("YOUR_API_KEY");
//!
//! // Single name: prediction fields read straight off the result via Deref.
//! let peter = client.genderize("peter", None).await?;
//! let gender = peter.gender.clone();
//! let remaining_after = peter.quota.remaining;
//!
//! // Batch: aggregate a list of names into a distribution.
//! let ages = client
//!     .agify_batch(&["michael", "matthew", "jane"], None)
//!     .await?;
//! let distribution: Vec<Option<i64>> = ages.results.iter().map(|p| p.age).collect();
//! let remaining = ages.quota.remaining;
//! # let _ = (gender, remaining_after, distribution, remaining);
//! # Ok(())
//! # }
//! ```
//!
//! The base URLs and the User-Agent are hardcoded constants. The constructor
//! takes a required API key and an optional timeout. An empty or blank key makes
//! every request fail with [`Error::Validation`] before any HTTP call.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "blocking")]
mod blocking;
mod client;
mod errors;
mod models;

#[cfg(feature = "blocking")]
pub use blocking::{BlockingDemografix, BlockingTransport, ReqwestBlockingTransport};
pub use client::{Demografix, RawResponse, Request, Transport};
pub use errors::Error;
pub use models::{
    AgifyPrediction, AgifyResult, Batch, GenderizePrediction, GenderizeResult, NationalizeCountry,
    NationalizePrediction, NationalizeResult, Quota,
};
