use axum::Form;
use axum::{extract::State, response::IntoResponse};
use axum::http::StatusCode;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use sailfish::TemplateOnce;

const COMPANY_NAME: &str = "nyantec GmbH";
const IMPRESSUM: &str = "https://nyantec.com/impressum/";

#[derive(TemplateOnce)]
#[template(path = "layout.stpl")]
struct Layout<B: TemplateOnce> {
	company_name: &'static str,
	body: B,
	impressum_link: &'static str,
}

#[derive(TemplateOnce)]
#[template(path = "main.stpl")]
struct MainPage {
	user: mail_passwd::User,
	passwords: Vec<mail_passwd::Password>
}

#[derive(TemplateOnce)]
#[template(path = "new_password.stpl")]
struct NewPasswordPage {
	password: String
}

#[derive(TemplateOnce)]
#[template(path = "deleted_password.stpl")]
struct DeletedPasswordPage;

type Service = mail_passwd::Service<mail_passwd::MigrationsDone>;
async fn mainpage(
	State(backend): State<Arc<Service>>,
	user: mail_passwd::User
) -> axum::response::Response {
	axum::response::Html(Layout {
		company_name: COMPANY_NAME,
		body: MainPage {
			passwords: match backend.list_passwords_for(&user).await {
				Ok(passwords) => passwords,
				Err(err) => return (
					StatusCode::INTERNAL_SERVER_ERROR,
					[("Content-Type", "text/plain")],
					format!("SQL layer error: {}", err)
				).into_response()
			},
			user,
		},
		impressum_link: IMPRESSUM
	}.render_once().unwrap()).into_response()
}

#[derive(serde::Deserialize)]
struct DeletePasswordForm {
	label: String
}

async fn delete_password(
	State(backend): State<Arc<Service>>,
	user: mail_passwd::User,
	Form(form): Form<DeletePasswordForm>
) -> axum::response::Response {
	match backend.rm_password_for(&user, &form.label).await {
		Ok(()) => axum::response::Html(Layout {
			company_name: COMPANY_NAME,
			body: DeletedPasswordPage,
			impressum_link: IMPRESSUM
		}.render_once().unwrap()).into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err)
		).into_response()
	}
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExpiresIn {
	NoExpiry,
	Week,
	Month,
	SixMonths,
	Year
}
#[derive(serde::Deserialize)]
struct CreatePasswordForm {
	label: String,
	expires_in: ExpiresIn
}

async fn create_password(
	State(backend): State<Arc<Service>>,
	user: mail_passwd::User,
	Form(form): Form<CreatePasswordForm>
) -> axum::response::Response {
	fn get_time_after_days(days: u64) -> chrono::DateTime<chrono::FixedOffset> {
		chrono::DateTime::<chrono::Utc>::from(
			std::time::SystemTime::now()
				+ std::time::Duration::from_secs(days * 60 * 60 * 24)
		).into()
	}

	match backend.new_password(&user, &form.label, match form.expires_in {
		ExpiresIn::NoExpiry => None,
		ExpiresIn::Week => Some(get_time_after_days(7)),
		ExpiresIn::Month => Some(get_time_after_days(30)),
		ExpiresIn::SixMonths => Some(get_time_after_days(30 * 6)),
		ExpiresIn::Year => Some(get_time_after_days(365)),
	}).await {
		Ok(password) => axum::response::Html(Layout {
			company_name: COMPANY_NAME,
			body: NewPasswordPage { password },
			impressum_link: IMPRESSUM
		}.render_once().unwrap()).into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err)
		).into_response()
	}
}


#[tokio::main]
async fn main() -> Result<(), hyper::Error> {
	let backend = match mail_passwd::Service::new({
		let database_url = match std::env::var("DATABASE_URL") {
			Ok(val) => val,
			Err(err) => panic!("DATABASE_URL not set or invalid: {}", err),
		};

		match PgPoolOptions::new().max_connections(5).connect(&database_url).await {
			Ok(db) => db,
			Err(err) => panic!("Connection to the database failed: {}", err),
		}
	})
	.run_migrations()
	.await
	{
		Ok(backend) => backend,
		Err(err) => panic!("Database migrations failed: {}", err),
	};

	let app = axum::Router::new()
		.route("/", axum::routing::get(mainpage))
		.route("/delete_password", axum::routing::post(delete_password))
		.route("/create_password", axum::routing::post(create_password))
		.with_state(Arc::new(backend));

	let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	hyper::server::Server::bind(&addr).serve(app.into_make_service()).await
}
