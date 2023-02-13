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
	http::request::Parts,
	response::IntoResponse,
	Form,
};
use chrono::{DateTime, FixedOffset};
use hyper::StatusCode;
use nyanpasswd::{axum::CertDn, Password, User};
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
