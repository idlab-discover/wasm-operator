//! A basic API client for interacting with the Kubernetes API
//!
//! The [`Client`] uses standard kube-rs-async error handling.
//!
//! This client can be used on its own or in conjuction with
//! the [`Api`][crate::api::Api] type for more structured
//! interaction with the kuberneres API

use crate::{error::ErrorResponse, Error, Result, abi};

use either::{Either, Left, Right};
use http::{self, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{self, Value};

/// Client for connecting with a Kubernetes cluster.
///
/// The best way to instantiate the client is either by
/// inferring the configuration from the environment using
/// [`Client::try_default`] or with an existing [`Config`]
/// using [`Client::new`]
#[derive(Clone)]
pub struct Client {
    default_ns: String,
}

impl Client {
    /// Create and initialize a [`Client`] using the given
    /// configuration.
    ///
    /// # Panics
    ///
    /// Panics if the configuration supplied leads to an invalid HTTP client.
    /// Refer to the [`reqwest::ClientBuilder::build`] docs for information
    /// on situations where this might fail. If you want to handle this error case
    /// use `Config::try_from` (note that this requires [`std::convert::TryFrom`]
    /// to be in scope.)
    pub fn new(default_ns: String) -> Self {
        Client { default_ns }
    }

    /// Create and initialize a [`Client`] using the inferred
    /// configuration.
    ///
    /// Will use [`Config::infer`] to try in-cluster enironment
    /// variables first, then fallback to the local kubeconfig.
    ///
    /// Will fail if neither configuration could be loaded.
    ///
    /// If you already have a [`Config`] then use `Client::try_from`
    /// instead
    pub async fn try_default() -> Result<Self> {
        Ok(Client::default())
    }

    fn send(&self, request: http::Request<Vec<u8>>) -> Result<http::Response<Vec<u8>>> {
        Ok(abi::execute_request(request))
    }

    /// Perform a raw HTTP request against the API and deserialize the response
    /// as JSON to some known type.
    pub fn request<T>(&self, request: http::Request<Vec<u8>>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let text = self.request_text(request)?;

        serde_json::from_str(&text).map_err(|e| {
            warn!("{}, {:?}", text, e);
            Error::SerdeError(e)
        })
    }

    /// Perform a raw HTTP request against the API and get back the response
    /// as a string
    pub fn request_text(&self, request: http::Request<Vec<u8>>) -> Result<String> {
        let method = request.method().to_string();
        let uri = request.uri().to_string();
        let res = self.send(request)?;
        trace!("{} {}: {:?}", method, uri, res.status());
        let s = res.status();

        let text = String::from_utf8(res.into_body()).expect("String should be UTF-8 encoded");
        handle_api_errors(&text, s)?;

        Ok(text)
    }

    /// Perform a raw HTTP request against the API and get back either an object
    /// deserialized as JSON or a [`Status`] Object.
    pub fn request_status<T>(&self, request: http::Request<Vec<u8>>) -> Result<Either<T, Status>>
    where
        T: DeserializeOwned,
    {
        let method = request.method().to_string();
        let uri = request.uri().to_string();
        let res = self.send(request)?;
        trace!("{} {}: {:?}", method, uri, res.status());
        let s = res.status();
        let text = String::from_utf8(res.into_body()).expect("String should be UTF-8 encoded");
        handle_api_errors(&text, s)?;

        // It needs to be JSON:
        let v: Value = serde_json::from_str(&text)?;
        if v["kind"] == "Status" {
            trace!("Status from {}", text);
            Ok(Right(serde_json::from_str::<Status>(&text).map_err(
                |e| {
                    warn!("{}, {:?}", text, e);
                    Error::SerdeError(e)
                },
            )?))
        } else {
            Ok(Left(serde_json::from_str::<T>(&text).map_err(|e| {
                warn!("{}, {:?}", text, e);
                Error::SerdeError(e)
            })?))
        }
    }
}

/// Kubernetes returned error handling
///
/// Either kube-rs-async returned an explicit ApiError struct,
/// or it someohow returned something we couldn't parse as one.
///
/// In either case, present an ApiError upstream.
/// The latter is probably a bug if encountered.
fn handle_api_errors(text: &str, s: StatusCode) -> Result<()> {
    if s.is_client_error() || s.is_server_error() {
        // Print better debug when things do fail
        // trace!("Parsing error: {}", text);
        if let Ok(errdata) = serde_json::from_str::<ErrorResponse>(text) {
            debug!("Unsuccessful: {:?}", errdata);
            Err(Error::Api(errdata))
        } else {
            warn!("Unsuccessful data error parse: {}", text);
            // Propagate errors properly via reqwest
            let ae = ErrorResponse {
                status: s.to_string(),
                code: s.as_u16(),
                message: format!("{:?}", text),
                reason: "Failed to parse error data".into(),
            };
            debug!("Unsuccessful: {:?} (reconstruct)", ae);
            Err(Error::Api(ae))
        }
    } else {
        Ok(())
    }
}

impl Default for Client {
    fn default() -> Self {
        Client {
            default_ns: "default".to_string(),
        }
    }
}

// TODO: replace with Status in k8s openapi?

/// A Kubernetes status object
#[allow(missing_docs)]
#[derive(Deserialize, Debug)]
pub struct Status {
    // TODO: typemeta
    // TODO: metadata that can be completely empty (listmeta...)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<StatusDetails>,
    #[serde(default, skip_serializing_if = "num::Zero::is_zero")]
    pub code: u16,
}

/// Status details object on the [`Status`] object
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct StatusDetails {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub causes: Vec<StatusCause>,
    #[serde(default, skip_serializing_if = "num::Zero::is_zero")]
    pub retry_after_seconds: u32,
}

/// Status cause object on the [`StatusDetails`] object
#[derive(Deserialize, Debug)]
#[allow(missing_docs)]
pub struct StatusCause {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
}
