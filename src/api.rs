/*
  Copyright © 2022 nyantec GmbH <oss@nyantec.com>
  Written by Vika Shleina <vsh@nyantec.com>
  
  Provided that these terms and disclaimer and all copyright notices
  are retained or reproduced in an accompanying document, permission
  is granted to deal in this work without restriction, including un‐
  limited rights to use, publicly perform, distribute, sell, modify,
  merge, give away, or sublicence.
  
  This work is provided "AS IS" and WITHOUT WARRANTY of any kind, to
  the utmost extent permitted by applicable law, neither express nor
  implied; without malicious intent or gross negligence. In no event
  may a licensor, author or contributor be held liable for indirect,
  direct, other damage, loss, or other issues arising in any way out
  of dealing in the work, even if advised of the possibility of such
  damage or existence of a defect, except proven that it results out
  of said person's immediate fault when using the work as intended.
 */
use std::sync::Arc;

use axum::{
	extract::State,
	http::StatusCode,
	response::{IntoResponse, Response},
	Json,
};

use crate::Service;

#[derive(serde::Deserialize)]
struct AuthenticationForm {
	user: String,
	password: String,
}

/// Check a user password and return one of the following responses:
/// - `200 OK` - password is correct
/// - `400 Bad Request` - this user is unknown
/// - `403 Forbidden` - login disabled by administrator
/// - `401 Unauthorized` - this password is either expired or it is not valid
/// - `500 Internal Server Error` - service suffered an internal error
async fn authenticate_user(State(db): State<Arc<Service>>, Json(form): Json<AuthenticationForm>) -> StatusCode {
	use nyanpasswd::AuthenticationResult as Auth;

	match db.verify_password(&form.user, &form.password).await {
		Ok(result) => match result {
			Auth::Ok => StatusCode::OK,
			Auth::NoSuchUser => StatusCode::BAD_REQUEST,
			Auth::LoginDisabled => StatusCode::FORBIDDEN,
			Auth::IncorrectPassword => StatusCode::UNAUTHORIZED,
		},
		Err(err) => {
			tracing::error!("Error verifying password: {}", err);
			StatusCode::INTERNAL_SERVER_ERROR
		}
	}
}

#[derive(serde::Deserialize)]
struct LookupForm {
	user: String,
}

/// Look up a user in the database and return some info about it. Return 404 if the user does not exist.
async fn lookup_user(State(db): State<Arc<Service>>, Json(form): Json<LookupForm>) -> Response {
	match db.find_user_by_name(&form.user).await {
		Ok(Some(user)) => {
			tracing::debug!("Replying with user data for {}: {:#?}", form.user, user);
			axum::response::Json(user).into_response()
		}
		Ok(None) => StatusCode::NOT_FOUND.into_response(),
		Err(err) => {
			tracing::error!("Error looking up user: {}", err);
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub fn router(backend: Arc<Service>) -> axum::Router {
	axum::Router::new()
		.route("/authenticate", axum::routing::post(authenticate_user))
		.route("/user_lookup", axum::routing::post(lookup_user))
		.with_state(backend)
}
