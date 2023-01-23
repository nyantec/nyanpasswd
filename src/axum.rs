use axum::extract::FromRequestParts;
use axum::http::{request::Parts, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::{MigrationsDone, Service, User};

lazy_static::lazy_static! {
	static ref DN_UID_REGEX: regex::Regex = regex::Regex::new(r#"UID ?= ?([a-z][a-z][a-z])"#).unwrap();
}

const ERROR_MESSAGE_TLS_PROXY_MISCONFIGURED: &str = "TLS-terminating reverse proxy is misconfigured: required headers not found.

Hint: if you use nginx, use:
    proxy_set_header X-SSL-Verify $ssl_client_verify;
    proxy_set_header X-SSL-Client-Dn $ssl_client_s_dn;
to provide the necessary headers.
";

pub struct CertDn(String);
impl CertDn {
	/// Parse the UID out of a client certificate DN.
	pub fn uid(&self) -> Option<&str> {
		DN_UID_REGEX.captures(&self.0).map(|c| c.get(1).unwrap().as_str())
	}
}

#[async_trait::async_trait]
impl<T> FromRequestParts<T> for CertDn
where
	T: Send + Sync,
{
	type Rejection = CertDnExtractionError;

	async fn from_request_parts(parts: &mut Parts, _: &T) -> Result<Self, Self::Rejection> {
		match parts
			.headers
			.get("X-SSL-Verify")
			.map(|s| String::from_utf8_lossy(s.as_bytes()))
			.as_deref()
		{
			Some("SUCCESS") => {}
			// We have no client certificate
			Some("NONE") => return Err(CertDnExtractionError::NoTlsCert),
			// Client certificate validation failed (e.g. it was revoked)
			Some(failed) => return Err(CertDnExtractionError::CertValidationFailed(failed.to_owned())),
			None => {
				#[cfg(debug_assertions)]
				return Ok(CertDn(
					"O = nyantec GmbH, CN = Vika Shleina, GN = Viktoriya, SN = Shleina, pseudonym = Vika, UID = vsh".to_string(),
				));
				#[cfg(not(debug_assertions))]
				return Err(CertDnExtractionError::ReverseProxyMisconfigured);
			}
		}
		if let Some(ssl_client_s_dn) = parts
			.headers
			.get("X-SSL-Client-Dn")
			.map(|s| String::from_utf8_lossy(s.as_bytes()))
		{
			return Ok(CertDn(ssl_client_s_dn.to_string()));
		} else {
			return Err(CertDnExtractionError::ReverseProxyMisconfigured);
		}
	}
}

#[derive(Debug, thiserror::Error)]
pub enum CertDnExtractionError {
	#[error("No TLS client certificate was provided")]
	NoTlsCert,
	#[error("Certificate validation {0}")]
	CertValidationFailed(String),
	#[error("Required headers `X-SSL-Verify` and/or `X-SSL-Client-Dn` not found")]
	ReverseProxyMisconfigured,
}

impl From<&CertDnExtractionError> for StatusCode {
	fn from(err: &CertDnExtractionError) -> Self {
		use CertDnExtractionError::*;
		match err {
			CertValidationFailed(_) => StatusCode::FORBIDDEN,
			ReverseProxyMisconfigured => StatusCode::INTERNAL_SERVER_ERROR,
			NoTlsCert => StatusCode::UNAUTHORIZED,
		}
	}
}

impl IntoResponse for CertDnExtractionError {
	fn into_response(self) -> Response {
		(
			StatusCode::from(&self),
			[("Content-Type", "text/plain")],
			match &self {
				Self::ReverseProxyMisconfigured => ERROR_MESSAGE_TLS_PROXY_MISCONFIGURED.to_string(),
				_ => self.to_string(),
			},
		)
			.into_response()
	}
}

#[derive(Debug, thiserror::Error)]
pub enum UserExtractionError {
	#[error("SQL layer error: {0}")]
	Sql(#[from] sqlx::Error),
	#[error("User not found in database")]
	UserNotFound,
	#[error("No UID field in TLS client certificate's Subject DN")]
	NoUidFieldInCert,
	#[error("Error parsing TLS client certificate data")]
	Certificate(#[from] CertDnExtractionError),
}
impl From<&UserExtractionError> for StatusCode {
	fn from(err: &UserExtractionError) -> Self {
		use UserExtractionError::*;
		match err {
			Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
			UserNotFound => StatusCode::UNAUTHORIZED,
			NoUidFieldInCert => StatusCode::BAD_REQUEST,
			Certificate(err) => StatusCode::from(err),
		}
	}
}
impl IntoResponse for UserExtractionError {
	fn into_response(self) -> Response {
		(StatusCode::from(&self), [("Content-Type", "text/plain")], self.to_string()).into_response()
	}
}

#[async_trait::async_trait]
impl FromRequestParts<Arc<Service<MigrationsDone>>> for User {
	type Rejection = UserExtractionError;

	async fn from_request_parts(parts: &mut Parts, db: &Arc<Service<MigrationsDone>>) -> Result<Self, Self::Rejection> {
		match CertDn::from_request_parts(parts, db).await {
			Ok(cert_dn) => match cert_dn.uid() {
				Some(uid) => match db.find_user_by_name(uid).await {
					Ok(Some(user)) => Ok(user),
					Ok(None) => Err(UserExtractionError::UserNotFound),
					Err(err) => Err(UserExtractionError::Sql(err)),
				},
				None => Err(UserExtractionError::NoUidFieldInCert),
			},
			#[cfg_attr(debug_assertions, allow(unused_variables))]
			Err(err) => {
				#[cfg(debug_assertions)]
				return Ok(db.find_user_by_name("vsh").await?.unwrap());
				#[cfg(not(debug_assertions))]
				return Err(err.into());
			}
		}
	}
}

#[cfg(test)]
mod test {
	#[test]
	fn test_uid_regex() {
		use super::DN_UID_REGEX;
		const VSH_DN: &str = "O = nyantec GmbH, CN = Vika Shleina, GN = Viktoriya, SN = Shleina, pseudonym = Vika, UID = vsh";
		let captures = dbg!(DN_UID_REGEX.captures(VSH_DN).unwrap());
		assert_eq!(captures.get(1).unwrap().as_str(), "vsh");
	}
}
