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
use nyanpasswd::{User, Alias};

#[derive(sailfish::TemplateOnce)]
#[template(path = "aliases.stpl")]
struct AliasesPage {
	aliases: Vec<(String, Vec<Uuid>)>,
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
		Ok(()) => (StatusCode::FOUND, [("Location", "/admin/aliases/")]).into_response(),
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
		Ok(()) => (StatusCode::FOUND, [("Location", "/admin/aliases/")]).into_response(),
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
