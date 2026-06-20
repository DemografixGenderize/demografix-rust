//! Error types for the Demografix client.
//!
//! Every fallible method returns `Result<T, Error>`. Non-2xx responses map by
//! status code to a typed variant carrying the status, the server message, and
//! the quota when the rate-limit headers are present.

use crate::models::Quota;
use std::fmt;

/// The single error type returned by every client method.
///
/// Variants map one to one to the cross-language error hierarchy:
///
/// - [`Error::Auth`] — status 401.
/// - [`Error::Subscription`] — status 402.
/// - [`Error::Validation`] — status 422, also raised client-side when a batch
///   exceeds 10 names.
/// - [`Error::RateLimit`] — status 429; quota is always populated.
/// - [`Error::Api`] — any other non-2xx status (the base error).
/// - [`Error::Transport`] — network failure, timeout, or a non-JSON body; status
///   and quota may be absent.
///
/// The enum is `#[non_exhaustive]`: match it with the [`status`](Error::status),
/// [`message`](Error::message), and [`quota`](Error::quota) accessors, or include
/// a wildcard arm so a future variant does not break your build.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Invalid or missing API key (401).
    Auth {
        /// HTTP status code.
        status: u16,
        /// The `error` string from the response body.
        message: String,
        /// Quota parsed from the response headers, when present.
        quota: Option<Quota>,
    },
    /// Subscription problem, such as an expired freebie (402).
    Subscription {
        /// HTTP status code.
        status: u16,
        /// The `error` string from the response body.
        message: String,
        /// Quota parsed from the response headers, when present.
        quota: Option<Quota>,
    },
    /// Invalid request parameters (422), or a batch over 10 names rejected
    /// client-side before any HTTP call.
    Validation {
        /// HTTP status code. Zero for the client-side batch-size rejection.
        status: u16,
        /// The `error` string from the response body, or a client-side message.
        message: String,
        /// Quota parsed from the response headers, when present.
        quota: Option<Quota>,
    },
    /// Rate limit reached (429). Quota is always populated; read `reset` to know
    /// how long to wait.
    RateLimit {
        /// HTTP status code.
        status: u16,
        /// The `error` string from the response body.
        message: String,
        /// Quota parsed from the response headers.
        quota: Quota,
    },
    /// Any other non-2xx status (the base error).
    Api {
        /// HTTP status code.
        status: u16,
        /// The `error` string from the response body.
        message: String,
        /// Quota parsed from the response headers, when present.
        quota: Option<Quota>,
    },
    /// Network failure, timeout, or a non-JSON body. Status and quota may be
    /// absent.
    Transport {
        /// The underlying failure description.
        message: String,
        /// HTTP status code, when one was received.
        status: Option<u16>,
        /// Quota parsed from the response headers, when present.
        quota: Option<Quota>,
    },
}

impl Error {
    /// The HTTP status code, when one is associated with this error.
    ///
    /// Returns `None` for transport failures without a response and for the
    /// client-side batch-size rejection.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Auth { status, .. }
            | Error::Subscription { status, .. }
            | Error::Validation { status, .. }
            | Error::Api { status, .. } => {
                if *status == 0 {
                    None
                } else {
                    Some(*status)
                }
            }
            Error::RateLimit { status, .. } => Some(*status),
            Error::Transport { status, .. } => *status,
        }
    }

    /// The server message, or the client-side message for the batch-size case.
    pub fn message(&self) -> &str {
        match self {
            Error::Auth { message, .. }
            | Error::Subscription { message, .. }
            | Error::Validation { message, .. }
            | Error::RateLimit { message, .. }
            | Error::Api { message, .. }
            | Error::Transport { message, .. } => message,
        }
    }

    /// The quota carried by this error, when the rate-limit headers were present.
    pub fn quota(&self) -> Option<&Quota> {
        match self {
            Error::Auth { quota, .. }
            | Error::Subscription { quota, .. }
            | Error::Validation { quota, .. }
            | Error::Api { quota, .. }
            | Error::Transport { quota, .. } => quota.as_ref(),
            Error::RateLimit { quota, .. } => Some(quota),
        }
    }

    /// Map an HTTP status, server message, and optional quota to a typed error.
    pub(crate) fn from_status(status: u16, message: String, quota: Option<Quota>) -> Error {
        match status {
            401 => Error::Auth {
                status,
                message,
                quota,
            },
            402 => Error::Subscription {
                status,
                message,
                quota,
            },
            422 => Error::Validation {
                status,
                message,
                quota,
            },
            429 => match quota {
                Some(quota) => Error::RateLimit {
                    status,
                    message,
                    quota,
                },
                // The contract guarantees quota on 429; treat its absence as a
                // transport-level anomaly rather than fabricating values.
                None => Error::Transport {
                    message,
                    status: Some(status),
                    quota: None,
                },
            },
            _ => Error::Api {
                status,
                message,
                quota,
            },
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status() {
            Some(status) => write!(f, "{} (status {})", self.message(), status),
            None => write!(f, "{}", self.message()),
        }
    }
}

impl std::error::Error for Error {}
