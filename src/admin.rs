use std::sync::Arc;

use axum::{
	extract::{FromRequestParts, Query, State},
	http::request::Parts,
	response::IntoResponse,
	Form,
};
use chrono::{DateTime, FixedOffset};
use hyper::StatusCode;
use mail_passwd::{axum::CertDn, Password, User};
use sailfish::TemplateOnce;
use uuid::Uuid;

use crate::{Layout, Service, COMPANY_NAME, IMPRESSUM};

pub struct Admin(String);
#[derive(thiserror::Error, Debug)]
pub enum AdminRejection {
	#[error("Not an administrator")]
	NotAnAdmin,
	#[error("No UID in certificate")]
	NoUidInCert,
	#[error("Certificate parsing error: {0}")]
	Certificate(#[from] mail_passwd::axum::CertDnExtractionError),
}
impl IntoResponse for AdminRejection {
	fn into_response(self) -> axum::response::Response {
		(
			match &self {
				Self::NotAnAdmin => StatusCode::FORBIDDEN,
				Self::NoUidInCert => StatusCode::UNAUTHORIZED,
				Self::Certificate(err) => StatusCode::from(err),
			},
			[("Content-Type", "text/plain")],
			self.to_string(),
		)
			.into_response()
	}
}
#[async_trait::async_trait]
impl<T> FromRequestParts<T> for Admin
where
	T: Send + Sync,
{
	type Rejection = AdminRejection;
	async fn from_request_parts(parts: &mut Parts, state: &T) -> Result<Self, Self::Rejection> {
		let dn = CertDn::from_request_parts(parts, state).await?;
		let uid = dn.uid().ok_or(Self::Rejection::NoUidInCert)?;

		// TODO(@vsh): should this be configurable in other ways?
		if std::env::var("ADMIN_UIDS").unwrap_or_default().split(' ').any(|a| a == uid) {
			Ok(Admin(uid.to_string()))
		} else {
			Err(Self::Rejection::NotAnAdmin)
		}
	}
}

#[derive(TemplateOnce)]
#[template(path = "admin.stpl")]
struct AdminPage {
	users: Vec<mail_passwd::User>,
}

async fn homepage(State(backend): State<Arc<Service>>) -> axum::response::Response {
	match backend.list_users().await {
		Ok(users) => axum::response::Html(
			Layout {
				company_name: COMPANY_NAME,
				impressum_link: IMPRESSUM,
				body: AdminPage { users },
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
enum ExpiryDate {
	NoExpiry,
	ExpiresAt(DateTime<FixedOffset>),
}

impl<'de> serde::Deserialize<'de> for ExpiryDate {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct Visitor;
		impl<'de> serde::de::Visitor<'de> for Visitor {
			type Value = ExpiryDate;

			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: serde::de::Error,
			{
				if v.is_empty() {
					Ok(ExpiryDate::NoExpiry)
				} else {
					v.parse()
						.map(ExpiryDate::ExpiresAt)
						.map_err(|_| E::invalid_value(serde::de::Unexpected::Str(v), &self))
				}
			}

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("either an empty string or an RFC3339 timestamp")
			}
		}

		deserializer.deserialize_str(Visitor)
	}
}

#[derive(serde::Deserialize)]
struct CreateUserForm {
	username: String,
	expires_at: ExpiryDate,
}

impl From<ExpiryDate> for Option<DateTime<FixedOffset>> {
	fn from(date: ExpiryDate) -> Self {
		match date {
			ExpiryDate::NoExpiry => None,
			ExpiryDate::ExpiresAt(date) => Some(date),
		}
	}
}

async fn create_user(State(backend): State<Arc<Service>>, Form(form): Form<CreateUserForm>) -> axum::response::Response {
	match backend.create_user(&form.username, form.expires_at.into()).await {
		Ok(uuid) => (
			StatusCode::CREATED,
			[("Location", format!("/admin/manage_user?uid={}", uuid))],
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
struct ManageUserQuery {
	uid: Uuid,
}

#[derive(TemplateOnce)]
#[template(path = "admin_manage_user.stpl")]
struct ManageUserPage {
	user: User,
	passwords: Vec<Password>,
}

async fn manage_user(State(backend): State<Arc<Service>>, Query(user): Query<ManageUserQuery>) -> axum::response::Response {
	match backend.get_user_by_id(user.uid).await {
		Ok(Some(user)) => match backend.list_passwords_for(&user).await {
			Ok(passwords) => axum::response::Html(
				Layout {
					company_name: COMPANY_NAME,
					impressum_link: IMPRESSUM,
					body: ManageUserPage { user, passwords },
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
		},
		Ok(None) => StatusCode::NOT_FOUND.into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response(),
	}
}

// Several following functions are activated from a single form.
#[derive(serde::Deserialize)]
struct ManageUserForm {
	uid: Uuid,
	expires_at: ExpiryDate,
}

async fn deactivate_user(State(backend): State<Arc<Service>>, Form(form): Form<ManageUserForm>) -> axum::response::Response {
	match backend.toggle_user_login_allowed(form.uid).await {
		Ok(()) => (
			StatusCode::CREATED,
			[("Location", format!("/admin/manage_user?uid={}", form.uid))],
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

async fn expire_user(State(backend): State<Arc<Service>>, Form(form): Form<ManageUserForm>) -> axum::response::Response {
	match backend.set_user_expiry_date(form.uid, form.expires_at.into()).await {
		Ok(()) => (
			StatusCode::CREATED,
			[("Location", format!("/admin/manage_user?uid={}", form.uid))],
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

pub fn router(backend: Arc<Service>) -> axum::Router {
	axum::Router::new()
		.route("/", axum::routing::get(homepage))
		.route("/create_user", axum::routing::post(create_user))
		.route("/manage_user", axum::routing::get(manage_user))
		.route("/expire_user", axum::routing::post(expire_user))
		.route("/deactivate_user", axum::routing::post(deactivate_user))
		.with_state(backend)
		.layer(axum::middleware::from_extractor::<Admin>())
}
