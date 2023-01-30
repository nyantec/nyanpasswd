use std::sync::Arc;

use axum::{
	extract::State,
	http::request::Parts,
	response::IntoResponse,
	Form,
};
use chrono::{DateTime, FixedOffset};
use hyper::StatusCode;
use mail_passwd::{axum::CertDn, Password, User};
use sailfish::TemplateOnce;
use uuid::Uuid;

use crate::{Layout, Service, COMPANY_NAME, IMPRESSUM, DeletedPasswordPage, NewPasswordPage};

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExpiresIn {
	NoExpiry,
	Week,
	Month,
	SixMonths,
	Year,
}

#[derive(serde::Deserialize)]
pub(crate) struct CreatePasswordForm {
	uuid: Uuid,
	label: String,
	expires_in: ExpiresIn,
}

pub(crate) async fn create_password(
State(backend): State<Arc<Service>>,
Form(form): Form<CreatePasswordForm>,
) -> axum::response::Response {
	// no use refactoring that, I will replace that
	// once I will get the new date-picker working
	fn get_time_after_days(days: u64) -> chrono::DateTime<chrono::FixedOffset> {
		chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now() + std::time::Duration::from_secs(days * 60 * 60 * 24))
			.into()
	}

	let user = match backend.get_user_by_id(form.uuid).await {
		Ok(Some(user)) => user,
		Ok(None) => return (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			"This user does not exist."
		)
			.into_response(),
		Err(err) => return (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response()
	};

	match backend
		.new_password(
			&user,
			&form.label,
			match form.expires_in {
				ExpiresIn::NoExpiry => None,
				ExpiresIn::Week => Some(get_time_after_days(7)),
				ExpiresIn::Month => Some(get_time_after_days(30)),
				ExpiresIn::SixMonths => Some(get_time_after_days(30 * 6)),
				ExpiresIn::Year => Some(get_time_after_days(365)),
			},
		)
		.await
	{
		Ok(password) => axum::response::Html(
			Layout {
				company_name: COMPANY_NAME,
				body: NewPasswordPage {
					password,
					prevlink: Some(format!("/admin/manage_user?uid={}", user.id))
				},
				impressum_link: IMPRESSUM,
			}
			.render_once()
			.unwrap(),
		)
		.into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response(),
	}


}

#[derive(serde::Deserialize)]
pub(crate) struct DeletePasswordForm {
	uuid: Uuid,
	label: String,
}

pub(crate) async fn delete_password(
	State(backend): State<Arc<Service>>,
	Form(form): Form<DeletePasswordForm>,
) -> axum::response::Response {
	let user = match backend.get_user_by_id(form.uuid).await {
		Ok(Some(user)) => user,
		Ok(None) => return (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			"This user does not exist."
		)
			.into_response(),
		Err(err) => return (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response()
	};

	match backend.rm_password_for(&user, &form.label).await {
		Ok(()) => axum::response::Html(
			Layout {
				company_name: COMPANY_NAME,
				body: DeletedPasswordPage {
					prevlink: Some(format!("/admin/manage_user?uid={}", user.id))
				},
				impressum_link: IMPRESSUM,
			}
			.render_once()
			.unwrap(),
		)
		.into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response(),
	}
}
