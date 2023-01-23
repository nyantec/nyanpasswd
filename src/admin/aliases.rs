use sailfish::TemplateOnce;
use std::{sync::Arc, collections::HashMap};

use axum::{
	extract::State,
	http::StatusCode,
	response::IntoResponse,
	Form,
};
use uuid::Uuid;

use crate::{Layout, Service, COMPANY_NAME, IMPRESSUM};
use mail_passwd::{User, Alias};

#[derive(sailfish::TemplateOnce)]
#[template(path = "aliases.stpl")]
struct AliasesPage {
	aliases: HashMap<String, Vec<Uuid>>,
	users: HashMap<Uuid, User>
}

async fn list_aliases(State(backend): State<Arc<Service>>) -> axum::response::Response {
	match backend.list_all_aliases().await {
		Ok(aliases) => {
			let users: HashMap<Uuid, User> = {
				let mut users = HashMap::new();
				match backend.list_users().await {
					Ok(rows) => users.extend(rows.into_iter().map(|user| (user.id, user))),
					Err(err) => return (
						StatusCode::INTERNAL_SERVER_ERROR,
						[("Content-Type", "text/plain")],
						format!("SQL layer error: {}", err),
					)
						.into_response()
				};

				users
			};

			axum::response::Html(
				Layout {
					company_name: COMPANY_NAME,
					impressum_link: IMPRESSUM,
					body: AliasesPage { aliases, users },
				}
				.render_once()
				.unwrap(),
			)
			.into_response()
		},
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err),
		)
			.into_response()
	}
}

async fn add_alias(State(backend): State<Arc<Service>>, Form(alias): Form<Alias>) -> axum::response::Response {
	match backend.add_alias(&alias).await {
		Ok(()) => StatusCode::RESET_CONTENT.into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err)
		)
		.into_response()
	}
}

async fn delete_alias(State(backend): State<Arc<Service>>, Form(alias): Form<Alias>) -> axum::response::Response {
	match backend.remove_alias(&alias).await {
		Ok(()) => StatusCode::RESET_CONTENT.into_response(),
		Err(err) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			[("Content-Type", "text/plain")],
			format!("SQL layer error: {}", err)
		)
		.into_response()
	}

}

pub fn router(backend: Arc<Service>) -> axum::Router {
	axum::Router::new()
		.route("/", axum::routing::get(list_aliases).post(add_alias))
		.route("/delete", axum::routing::post(delete_alias))
		.with_state(backend)
}
