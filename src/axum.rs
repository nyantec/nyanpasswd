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

#[derive(Debug, thiserror::Error)]
pub enum UserExtractionError {
	#[error("SQL layer error: {0}")]
	Sql(#[from] sqlx::Error),
	#[error("User not found in database")]
	UserNotFound,
	#[error("No TLS client certificate was provided")]
	NoTlsCert,
	#[error("Certificate validation {0}")]
	CertValidationFailed(String),
	#[error("Required headers `X-SSL-Verify` and/or `X-SSL-Client-Dn` not found")]
	ReverseProxyMisconfigured,
	#[error("No UID field in TLS client certificate's Subject DN")]
	NoUidFieldInCert,
}

impl IntoResponse for UserExtractionError {
	fn into_response(self) -> Response {
		(
			match &self {
				UserExtractionError::Sql(err) => StatusCode::INTERNAL_SERVER_ERROR,
				UserExtractionError::UserNotFound => StatusCode::UNAUTHORIZED,
				UserExtractionError::NoTlsCert => StatusCode::UNAUTHORIZED,
				UserExtractionError::CertValidationFailed(_) => StatusCode::FORBIDDEN,
				UserExtractionError::ReverseProxyMisconfigured => StatusCode::INTERNAL_SERVER_ERROR,
				UserExtractionError::NoUidFieldInCert => StatusCode::BAD_REQUEST,
			},
			[("Content-Type", "text/plain")],
			match &self {
				Self::ReverseProxyMisconfigured => ERROR_MESSAGE_TLS_PROXY_MISCONFIGURED.to_string(),
				_ => self.to_string(),
			},
		)
			.into_response()
	}
}

#[async_trait::async_trait]
impl FromRequestParts<Arc<Service<MigrationsDone>>> for User {
	type Rejection = UserExtractionError;

	async fn from_request_parts(parts: &mut Parts, db: &Arc<Service<MigrationsDone>>) -> Result<Self, Self::Rejection> {
		match parts
			.headers
			.get("X-SSL-Verify")
			.map(|s| String::from_utf8_lossy(s.as_bytes()))
			.as_deref()
		{
			Some("SUCCESS") => {}
			// We have no client certificate
			Some("NONE") => return Err(UserExtractionError::NoTlsCert),
			// Client certificate validation failed (e.g. it was revoked)
			Some(failed) => return Err(UserExtractionError::CertValidationFailed(failed.to_owned())),
			None => {
				#[cfg(debug_assertions)]
				return Ok(db.find_user_by_name("vsh").await?.unwrap());
				#[cfg(not(debug_assertions))]
				return Err(UserExtractionError::ReverseProxyMisconfigured);
			}
		}
		if let Some(ssl_client_s_dn) = parts
			.headers
			.get("X-SSL-Client-Dn")
			.map(|s| String::from_utf8_lossy(s.as_bytes()))
		{
			// XXX: This only supports three-letter usernames!
			// A proper RFC4514-compliant parser is recommended
			if let Some(captures) = DN_UID_REGEX.captures(ssl_client_s_dn.as_ref()) {
				let username = captures.get(1).unwrap().as_str();
				tracing::debug!("UID from X-SSL-Client-Dn: {}", username);

				match db.find_user_by_name(username).await {
					Ok(Some(user)) => return Ok(user),
					Ok(None) => return Err(UserExtractionError::UserNotFound),
					Err(err) => return Err(UserExtractionError::Sql(err)),
				}
			} else {
				return Err(UserExtractionError::NoUidFieldInCert);
			}
		} else {
			#[cfg(debug_assertions)]
			return Ok(db.find_user_by_name("vsh").await?.unwrap());
			#[cfg(not(debug_assertions))]
			return Err(UserExtractionError::ReverseProxyMisconfigured);
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
