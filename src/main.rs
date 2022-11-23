use axum::http::StatusCode;
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use std::{str::FromStr, sync::Arc};
use tracing::error;

// >= 22 alphanumeric chars => >= 128 bits of entropy
const PASSWORD_LENGTH: usize = 64;

async fn webpage() -> axum::response::Html<&'static str> {
	axum::response::Html(include_str!("index.html"))
}

#[derive(Deserialize)]
struct PasswdChangeForm {
	username: String,
	old_passwd: String,
}

#[derive(thiserror::Error, Debug)]
enum Error {
	IO(#[from] std::io::Error),
	Bcrypt(#[from] bcrypt::BcryptError),
	InvalidUsername,
	InvalidPassword,
	Conflict,
}
impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		use Error::*;

		match self {
			IO(e) => write!(f, "i/o error: {}", e),
			Bcrypt(e) => write!(f, "bcrypt error: {}", e),
			InvalidUsername => write!(f, "invalid or non-existent username"),
			InvalidPassword => write!(f, "incorrect password"),
			Conflict => write!(f, "another password change is currently in progress"),
		}
	}
}
impl axum::response::IntoResponse for Error {
	fn into_response(self) -> axum::response::Response {
		(
			match self {
				Error::IO(_) => StatusCode::INTERNAL_SERVER_ERROR,
				Error::Bcrypt(_) => StatusCode::INTERNAL_SERVER_ERROR,
				Error::InvalidUsername => StatusCode::UNAUTHORIZED,
				Error::InvalidPassword => StatusCode::UNAUTHORIZED,
				Error::Conflict => StatusCode::CONFLICT,
			},
			[("Content-Type", "text/plain")],
			self.to_string(),
		)
			.into_response()
	}
}

async fn passwd(passwd: &PasswdChangeForm, config: &Config) -> Result<String, Error> {
	if passwd.username.len() != 3 || !passwd.username.chars().all(|c| c.is_ascii_alphabetic()) {
		return Err(Error::InvalidUsername);
	}

	let passwdfile = config.passwd_file_dir.join(&passwd.username);
	let old_hash = match tokio::fs::read_to_string(&passwdfile).await {
		Ok(hash) => hash,
		Err(e) => {
			if e.kind() == std::io::ErrorKind::NotFound {
				return Err(Error::InvalidUsername);
			} else {
				error!("Error reading password hash: {}", e);
				return Err(e.into());
			}
		}
	};

	match bcrypt::verify(&passwd.old_passwd, &old_hash) {
		Ok(true) => {
			let tmp = config.passwd_file_dir.join(format!("{}.tmp", &passwd.username));
			let mut tmpfile = match tokio::fs::OpenOptions::new().create_new(true).write(true).open(&tmp).await {
				Ok(file) => file,
				Err(e) => match e.kind() {
					std::io::ErrorKind::AlreadyExists => return Err(Error::Conflict),
					_ => {
						error!("Error creating tempfile for password hash: {}", e);
						return Err(e.into());
					}
				},
			};

			// SAFETY: the Alphanumeric distribution only produces
			// ASCII alphanumeric characters, which are valid UTF-8
			let new_passwd: String = unsafe {
				String::from_utf8_unchecked(
					rand::thread_rng()
						.sample_iter(&Alphanumeric)
						.take(PASSWORD_LENGTH)
						.collect::<Vec<u8>>(),
				)
			};

			let new_hash = bcrypt::hash(new_passwd.as_str(), config.bcrypt_cost)?;

			use tokio::io::AsyncWriteExt;

			if let Err(e) = tmpfile.write_all(new_hash.as_bytes()).await {
				eprintln!("Error writing password to tempfile: {}", e);
				return Err(e.into());
			}
			// XXX fsync the directory too! Rust currently can't do that
			// <https://github.com/tokio-rs/tokio/issues/1922>
			tmpfile.sync_all().await?;
			tmpfile.shutdown().await?;
			drop(tmpfile);

			tokio::fs::rename(tmp, passwdfile).await?;

			Ok(new_passwd)
		}
		Ok(false) => Err(Error::InvalidPassword),
		Err(e) => {
			eprintln!("Error verifying password hash: {}", e);
			Err(e.into())
		}
	}
}

async fn passwd_handler(
	axum::extract::Form(form): axum::extract::Form<PasswdChangeForm>,
	axum::Extension(config): axum::Extension<Arc<Config>>,
) -> axum::response::Response {
	use axum::response::IntoResponse;

	match passwd(&form, &config).await {
		Ok(password) => (StatusCode::OK, [("Content-Type", "text/plain")], password).into_response(),
		Err(e) => {
			let tempfile = config.passwd_file_dir.join(format!("{}.tmp", &form.username));
			tokio::fs::remove_file(tempfile).await.unwrap();
			e.into_response()
		}
	}
}

struct Config {
	passwd_file_dir: std::path::PathBuf,
	bcrypt_cost: u32,
}

#[tokio::main]
async fn main() -> Result<(), hyper::Error> {
	let config = Config {
		passwd_file_dir: if let Some(path) = std::env::var_os("PASSWD_FILE_DIR") {
			path.into()
		} else {
			eprintln!("Set `PASSWD_FILE_DIR` to a directory with password hash files named after users!");
			std::process::exit(1)
		},
		bcrypt_cost: match std::env::var("BCRYPT_COST")
			.map(|v| u32::from_str(&v))
			.unwrap_or(Ok(bcrypt::DEFAULT_COST))
		{
			Ok(num) => {
				if (4..=31).contains(&num) {
					num
				} else {
					eprintln!("BCRYPT_COST should be a number between 4 and 31!");
					std::process::exit(1)
				}
			}
			Err(err) => {
				eprintln!("Error parsing BCRYPT_COST: {}", err);
				std::process::exit(1);
			}
		},
	};

	let app = axum::Router::new()
		.route("/", axum::routing::get(webpage).post(passwd_handler))
		.layer(axum::Extension(Arc::new(config)));

	let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
	hyper::server::Server::bind(&addr).serve(app.into_make_service()).await
}
