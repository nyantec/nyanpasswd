use axum::http::StatusCode;
use axum::extract::State;
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use std::{str::FromStr, sync::Arc};
use mail_passwd::Service;
use tracing::error;
use sqlx::postgres::PgPoolOptions;

async fn mainpage() {}

#[tokio::main]
async fn main() -> Result<(), hyper::Error> {
	let backend = match Service::new({
		let database_url = match std::env::var("DATABASE_URL") {
			Ok(val) => val,
			Err(err) => panic!("DATABASE_URL not set or invalid: {}", err)
		};

		match PgPoolOptions::new()
			.max_connections(5)
			.connect(&database_url)
			.await
		{
			Ok(db) => db,
			Err(err) => panic!("Connection to the database failed: {}", err)
		}
	}).run_migrations().await {
		Ok(backend) => backend,
		Err(err) => panic!("Database migrations failed: {}", err)
	};

	let app = axum::Router::new()
		//.route("/", axum::routing::get(webpage).post(passwd_handler))
		.with_state(Arc::new(backend));

	let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	hyper::server::Server::bind(&addr).serve(app.into_make_service()).await
}
