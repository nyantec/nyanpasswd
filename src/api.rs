use std::sync::Arc;

use axum::{http::StatusCode, response::{Response, IntoResponse}, extract::State, Json};

use crate::Service;

#[derive(serde::Deserialize)]
struct AuthenticationForm {
	user: String,
	password: String
}

/// Check a user password and return one of the following responses:
/// - `200 OK` - password is correct
/// - `400 Bad Request` - this user is unknown
/// - `403 Forbidden` - login disabled by administrator
/// - `401 Unauthorized` - this password is either expired or it is not valid
/// - `500 Internal Server Error` - service suffered an internal error
async fn authenticate_user(State(db): State<Arc<Service>>, Json(form): Json<AuthenticationForm>) -> StatusCode {
	use mail_passwd::AuthenticationResult as Auth;

	match db.verify_password(&form.user, &form.password).await {
		Ok(result) => match result {
			Auth::Ok => StatusCode::OK,
			Auth::NoSuchUser => StatusCode::BAD_REQUEST,
			Auth::LoginDisabled => StatusCode::FORBIDDEN,
			Auth::IncorrectPassword => StatusCode::UNAUTHORIZED
		},
		Err(err) => {
			tracing::error!("Error verifying password: {}", err);
			StatusCode::INTERNAL_SERVER_ERROR
		}
	}
}

#[derive(serde::Deserialize)]
struct LookupForm {
	user: String
}

/// Look up a user in the database and return some info about it. Return 404 if the user does not exist.
async fn lookup_user(State(db): State<Arc<Service>>, Json(form): Json<LookupForm>) -> Response {
	match db.find_user_by_name(&form.user).await {
		Ok(Some(user)) => axum::response::Json(user).into_response(),
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
