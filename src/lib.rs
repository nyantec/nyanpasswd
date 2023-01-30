use futures::StreamExt;
use argon2::{
	password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
	Argon2,
};
use uuid::Uuid;

mod util {
	use rand::distributions::Alphanumeric;
	use rand::{CryptoRng, Rng};

	const PASSWORD_LENGTH: usize = 64;

	pub(super) fn gen_password<R: Rng + CryptoRng>(rng: &mut R) -> String {
		// SAFETY: the Alphanumeric distribution only produces ASCII alphanumeric characters, which are valid UTF-8
		unsafe { String::from_utf8_unchecked(rng.sample_iter(&Alphanumeric).take(PASSWORD_LENGTH).collect::<Vec<u8>>()) }
	}
}

pub mod axum;

#[derive(sqlx::FromRow, Debug)]
pub struct Password {
	pub userid: Uuid,
	pub label: String,
	pub hash: String,
	pub created_at: chrono::DateTime<chrono::FixedOffset>,
	pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(sqlx::FromRow, Debug, serde::Serialize)]
pub struct User {
	pub id: Uuid,
	pub username: String,
	pub login_allowed: bool,
	pub created_at: chrono::DateTime<chrono::FixedOffset>,
	pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	pub non_human: bool
}

#[derive(sqlx::FromRow, Debug, serde::Serialize, serde::Deserialize)]
pub struct Alias {
	pub alias_name: String,
	pub destination: Uuid,
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

mod sealed {
	use std::fmt::Debug;

	#[derive(Debug)]
	pub enum MigrationsDone {}
	#[derive(Debug)]
	pub enum Created {}
	pub trait InitState: Debug {}
	impl InitState for super::MigrationsDone {}
	impl InitState for super::Created {}
}
use sealed::Created;
pub use sealed::MigrationsDone;

#[derive(Clone)]
pub struct Service<S: sealed::InitState> {
	db: sqlx::PgPool,
	argon2: Argon2<'static>,
	_migrations: std::marker::PhantomData<S>,
}

// We implement Debug manually, because:
// - Argon2<'a> doesn't implement Debug
// - Showing the phantom data is absolutely useless here
impl<T: sealed::InitState> std::fmt::Debug for Service<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Service").field("db", &self.db).finish_non_exhaustive()
	}
}

pub enum AuthenticationResult {
	Ok,
	NoSuchUser,
	LoginDisabled,
	IncorrectPassword,
}

impl Service<Created> {
	pub fn new(db: sqlx::PgPool) -> Self {
		Self {
			db,
			argon2: argon2::Argon2::new(argon2::Algorithm::Argon2i, Default::default(), Default::default()),
			_migrations: std::marker::PhantomData,
		}
	}

	#[tracing::instrument]
	pub async fn run_migrations(self) -> sqlx::Result<Service<MigrationsDone>> {
		MIGRATOR.run(&self.db).await?;

		Ok(Service::<MigrationsDone> {
			_migrations: std::marker::PhantomData::<MigrationsDone>,
			db: self.db,
			argon2: self.argon2,
		})
	}
}

impl Service<MigrationsDone> {
	/// Generate a password for a user designated by `user` and save it with the corresponding `label`.
	#[tracing::instrument]
	pub async fn new_password(
		&self,
		user: &User,
		label: &str,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	) -> sqlx::Result<String> {
		let mut rng = rand::rngs::OsRng;
		let password: String = self::util::gen_password(&mut rng);

		sqlx::query("INSERT INTO passdb (userid, label, hash, expires_at) VALUES ($1, $2, $3, $4)")
			.bind(user.id)
			.bind(label)
			.bind(
				self.argon2
					.hash_password(password.as_bytes(), &SaltString::generate(&mut rng))
					.unwrap()
					.to_string(),
			)
			.bind(expires_at)
			.execute(&self.db)
			.await?;

		Ok(password)
	}
	/// Irreversibly remove a password designated by `label` from the specified user.
	#[tracing::instrument]
	pub async fn rm_password_for(&self, user: &User, label: &str) -> sqlx::Result<()> {
		sqlx::query("DELETE FROM passdb WHERE userid = $1 AND label = $2")
			.bind(&user.id)
			.bind(&label)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	/// List passwords owned by the current user.
	#[tracing::instrument]
	pub async fn list_passwords_for_username(&self, user: &str) -> sqlx::Result<Vec<Password>> {
		sqlx::query_as::<_, Password>(
			"SELECT passdb.* FROM passdb INNER JOIN userdb ON userdb.id = userid WHERE userdb.username = $1",
		)
		.bind(user)
		.fetch_all(&self.db)
		.await
	}

	#[tracing::instrument]
	pub async fn list_passwords_for(&self, user: &User) -> sqlx::Result<Vec<Password>> {
		sqlx::query_as::<_, Password>("SELECT passdb.* FROM passdb WHERE userid = $1 ORDER BY passdb.label")
			.bind(user.id)
			.fetch_all(&self.db)
			.await
	}

	/// Verify a password for a user identified by their username.
	#[tracing::instrument]
	pub async fn verify_password(&self, user: &str, password: &str) -> sqlx::Result<AuthenticationResult> {
		// First, wrap things in a transaction. This is because we need extreme granularity in
		// errors that might be hard to do in a single SELECT statement, but with multiple SELECT
		// statements, we need consistency. This is provided by REPEATABLE READ transaction
		// isolation level.
		//
		// See <https://www.postgresql.org/docs/current/transaction-iso.html> for more info.
		let mut txn = self.db.begin().await?;
		sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
			.execute(&mut txn)
			.await?;
		// First, check if user exists and is allowed to log in.
		match sqlx::query_as::<_, (bool,)>("SELECT login_allowed FROM userdb WHERE username = $1")
			.bind(user)
			.fetch_optional(&mut txn)
			.await
		{
			Ok(Some((login_allowed,))) => {
				if !login_allowed {
					return Ok(AuthenticationResult::LoginDisabled);
				}
			}
			Ok(None) => return Ok(AuthenticationResult::NoSuchUser),
			Err(err) => return Err(err),
		};

		let mut stream = sqlx::query_scalar::<_, String>(
			"SELECT passdb.hash FROM passdb INNER JOIN userdb ON userid = userdb.id WHERE userdb.username = $1",
		)
		.bind(user)
		.fetch_many(&self.db);

		while let Some(result) = stream.next().await {
			match result {
				Ok(sqlx::Either::Right(hash)) => {
					if self
						.argon2
						.verify_password(password.as_bytes(), &PasswordHash::new(&hash).expect("hash should be valid"))
						.is_ok()
					{
						txn.commit().await?;
						return Ok(AuthenticationResult::Ok);
					}
				}
				Err(err) => {
					txn.commit().await?;
					return Err(err);
				}
				Ok(sqlx::Either::Left(_query_result)) => {}
			}
		}
		txn.commit().await?;
		Ok(AuthenticationResult::IncorrectPassword)
	}
	/// Resolve a user by its username.
	#[tracing::instrument]
	pub async fn find_user_by_name(&self, username: &str) -> sqlx::Result<Option<User>> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb WHERE username = $1")
			.bind(username)
			.fetch_optional(&self.db)
			.await
	}
	/// Find a user by its static neverchanging UUID.
	#[tracing::instrument]
	pub async fn get_user_by_id(&self, uuid: Uuid) -> sqlx::Result<Option<User>> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb WHERE id = $1")
			.bind(uuid)
			.fetch_optional(&self.db)
			.await
	}
	/// List all users.
	#[tracing::instrument]
	pub async fn list_users(&self) -> sqlx::Result<Vec<User>> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb ORDER BY username").fetch_all(&self.db).await
	}
	/// Create a new user.
	#[tracing::instrument]
	pub async fn create_user(
		&self,
		username: &str,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
		non_human: bool,
	) -> sqlx::Result<Uuid> {
		sqlx::query_scalar::<_, Uuid>("INSERT INTO userdb (username, expires_at, non_human) VALUES ($1, $2, $3) RETURNING id")
			.bind(username)
			.bind(expires_at)
			.bind(non_human)
			.fetch_one(&self.db)
			.await
	}
	/// Activate or deactivate a user's login capabilities.
	#[tracing::instrument]
	pub async fn toggle_user_login_allowed(&self, user: Uuid) -> sqlx::Result<()> {
		sqlx::query("UPDATE userdb SET login_allowed = NOT login_allowed WHERE id = $1")
			.bind(user)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	/// Set user expiry date.
	#[tracing::instrument]
	pub async fn set_user_expiry_date(
		&self,
		user: Uuid,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	) -> sqlx::Result<()> {
		sqlx::query("UPDATE userdb SET expires_at = $2 WHERE id = $1")
			.bind(user)
			.bind(expires_at)
			.execute(&self.db)
			.await?;

		Ok(())
	}

	pub async fn add_alias(&self, alias: &Alias) -> sqlx::Result<()> {
		sqlx::query("INSERT INTO aliases (alias_name, destination) VALUES ($1, $2)")
			.bind(alias.alias_name.as_str())
			.bind(alias.destination)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	pub async fn remove_alias(&self, alias: &Alias) -> sqlx::Result<()> {
		sqlx::query("DELETE FROM aliases WHERE alias_name = $1 AND destination = $2")
			.bind(alias.alias_name.as_str())
			.bind(alias.destination)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	pub async fn list_all_aliases(&self) -> sqlx::Result<Vec<(String, Vec<Uuid>)>> {
		sqlx::query_as::<_, (String, Vec<Uuid>)>("SELECT alias_name, array_agg(destination) FROM aliases GROUP BY alias_name ORDER BY alias_name")
			.fetch_all(&self.db)
			.await
	}
}

#[cfg(test)]
mod test {
	use super::AuthenticationResult;
	use futures::{StreamExt, TryStreamExt};

	fn create_service(pool: sqlx::PgPool) -> crate::Service<super::MigrationsDone> {
		// Note: you are DEFINITELY NOT SUPPOSED to be creating this
		// object like that! SQLx test harness automatically applies
		// migrations, so we don't need to run them a second time.
		//
		// Since the typestate field is private, you won't be able to
		// do it in external code anyway.
		crate::Service::<super::MigrationsDone> {
			_migrations: std::marker::PhantomData::<super::MigrationsDone>,
			db: pool,
			argon2: argon2::Argon2::new(argon2::Algorithm::Argon2i, Default::default(), Default::default()),
		}
	}

	#[sqlx::test]
	async fn smoke_test(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);

		let uuid = svc.create_user("vsh", None, false).await?;
		let user = svc.find_user_by_name("vsh").await?.unwrap();
		assert_eq!(user.id, uuid);

		// Check that no passwords are defined
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 0);
		assert!(matches!(
			svc.verify_password(&user.username, "AAAAAAAA").await?,
			AuthenticationResult::IncorrectPassword
		));
		// Generate a password and ensure it matches
		let password = svc.new_password(&user, "longiflorum", None).await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 1);
		assert!(matches!(
			svc.verify_password(&user.username, &password).await?,
			AuthenticationResult::Ok
		));
		// Ensure something else isn't accepted
		assert!(matches!(
			svc.verify_password(&user.username, "AAAAAAAA").await?,
			AuthenticationResult::IncorrectPassword
		));
		// Ensure non-unique labels are rejected for the same user
		svc.new_password(&user, "longiflorum", None).await.unwrap_err();
		// Generate another password and check if it works
		let another_password = svc.new_password(&user, "primrose", None).await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 2);
		assert!(matches!(
			svc.verify_password(&user.username, &another_password).await?,
			AuthenticationResult::Ok
		));
		// Check that the older password still works
		assert!(matches!(
			svc.verify_password(&user.username, &password).await?,
			AuthenticationResult::Ok
		));
		// Remove a password
		svc.rm_password_for(&user, "longiflorum").await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 1);
		assert!(matches!(
			svc.verify_password(&user.username, &password).await?,
			AuthenticationResult::IncorrectPassword
		));
		assert!(matches!(
			svc.verify_password(&user.username, &another_password).await?,
			AuthenticationResult::Ok
		));

		Ok(())
	}

	#[sqlx::test]
	async fn test_login_allowed(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);

		// Create a user
		let uuid = svc.create_user("vsh", None, false).await?;
		let user = svc.find_user_by_name("vsh").await?.unwrap();
		assert_eq!(user.id, uuid);
		// Create a password for them
		let password = svc.new_password(&user, "longiflorum", None).await?;
		// Check that they can log in
		assert!(matches!(
			svc.verify_password("vsh", &password).await?,
			AuthenticationResult::Ok
		));
		// Disallow this user to log in
		svc.toggle_user_login_allowed(uuid).await?;
		// Check they can't log in
		assert!(matches!(
			svc.verify_password("vsh", &password).await?,
			AuthenticationResult::LoginDisabled
		));
		// Allow this user back
		svc.toggle_user_login_allowed(uuid).await?;
		// Ensure they're able to log in again
		assert!(matches!(
			svc.verify_password("vsh", &password).await?,
			AuthenticationResult::Ok
		));

		Ok(())
	}

	#[sqlx::test]
	async fn test_non_existent_user(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);
		assert!(svc.find_user_by_name("vsh").await.unwrap().is_none());

		Ok(())
	}

	#[sqlx::test]
	async fn test_aliases(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);

		let users = futures::stream::iter(["vsh", "mvs", "mak"])
			.then(|user| svc.create_user(user, None, false))
			.try_collect::<Vec<uuid::Uuid>>()
			.await?;

		for user in &users {
			svc.add_alias(&super::Alias {
				alias_name: "ops".to_string(),
				destination: *user,
			})
			.await?;
		}

		assert_eq!(svc.list_all_aliases().await?, vec![("ops".to_owned(), users.to_vec())]);

		Ok(())
	}
}
