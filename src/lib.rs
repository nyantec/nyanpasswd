use argon2::{
	password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
	Argon2,
};
use futures::StreamExt;
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

mod axum;

#[derive(sqlx::FromRow, Debug)]
pub struct Password {
	pub userid: Uuid,
	pub label: String,
	pub hash: String,
	pub created_at: chrono::DateTime<chrono::FixedOffset>,
	pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(sqlx::FromRow, Debug)]
pub struct User {
	pub id: Uuid,
	pub username: String,
	pub login_allowed: bool,
	pub created_at: chrono::DateTime<chrono::FixedOffset>,
	pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

mod sealed {
	pub enum MigrationsDone {}
	pub enum Created {}
	pub trait InitState {}
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

impl Service<Created> {
	pub fn new(db: sqlx::PgPool) -> Self {
		Self {
			db,
			argon2: argon2::Argon2::new(argon2::Algorithm::Argon2i, Default::default(), Default::default()),
			_migrations: std::marker::PhantomData,
		}
	}

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
	pub async fn new_password(
		&self,
		user: &User,
		label: &str,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	) -> sqlx::Result<String> {
		let mut rng = rand::rngs::OsRng;
		let password: String = self::util::gen_password(&mut rng);

		sqlx::query(
			"INSERT INTO passdb (userid, label, hash, expires_at) VALUES ($1, $2, $3, $4)",
		)
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
	pub async fn rm_password_for(&self, user: &User, label: &str) -> sqlx::Result<()> {
		sqlx::query("DELETE FROM passdb WHERE userid = $1 AND label = $2")
			.bind(&user.id)
			.bind(&label)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	/// List passwords owned by the current user.
	pub async fn list_passwords_for_username(&self, user: &str) -> sqlx::Result<Vec<Password>> {
		sqlx::query_as::<_, Password>(
			"SELECT passdb.* FROM passdb INNER JOIN userdb ON userdb.id = userid WHERE userdb.username = $1",
		)
		.bind(user)
		.fetch_all(&self.db)
		.await
	}
	pub async fn list_passwords_for(&self, user: &User) -> sqlx::Result<Vec<Password>> {
		sqlx::query_as::<_, Password>(
			"SELECT passdb.* FROM passdb WHERE userid = $1",
		)
		.bind(user.id)
		.fetch_all(&self.db)
		.await
	}

	/// Verify a password for a user identified by their username.
	pub async fn verify_password(&self, user: &str, password: &str) -> sqlx::Result<bool> {
		let mut stream = sqlx::query_scalar::<_, String>(
			"SELECT passdb.hash FROM passdb INNER JOIN userdb ON userid = userdb.id WHERE userdb.username = $1 AND userdb.login_allowed = true",
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
						return Ok(true);
					}
				}
				Err(err) => return Err(err),
				Ok(sqlx::Either::Left(_query_result)) => {}
			}
		}

		Ok(false)
	}
	/// Resolve a user by its username.
	pub async fn find_user_by_name(&self, username: &str) -> sqlx::Result<Option<User>> {
		match sqlx::query_as::<_, User>("SELECT * FROM userdb WHERE username = $1")
			.bind(username)
			.fetch_one(&self.db)
			.await
		{
			Ok(user) => Ok(Some(user)),
			Err(sqlx::error::Error::RowNotFound) => Ok(None),
			Err(err) => Err(err)
		}

	}
	/// List all users.
	pub async fn list_users(&self) -> sqlx::Result<Vec<User>> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb").fetch_all(&self.db).await
	}
	/// Create a new user.
	pub async fn create_user(
		&self,
		username: &str,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	) -> sqlx::Result<Uuid> {
		sqlx::query_scalar::<_, Uuid>("INSERT INTO userdb (username, expires_at) VALUES ($1, $2) RETURNING id")
			.bind(username)
			.bind(expires_at)
			.fetch_one(&self.db)
			.await
	}
	/// Activate or deactivate a user's login capabilities.
	pub async fn set_user_login_allowed(&self, user: &str, login_allowed: bool) -> sqlx::Result<()> {
		sqlx::query("UPDATE userdb SET login_allowed = $2 WHERE username = $1")
			.bind(user)
			.bind(login_allowed)
			.execute(&self.db)
			.await?;

		Ok(())
	}
}

#[cfg(test)]
mod test {
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

		let uuid = svc.create_user("vsh", None).await?;
		let user = svc.find_user_by_name("vsh").await?.unwrap();
		assert_eq!(user.id, uuid);

		// Check that no passwords are defined
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 0);
		assert!(!svc.verify_password(&user.username, "AAAAAAAA").await?);
		// Generate a password and ensure it matches
		let password = svc.new_password(&user, "longiflorum", None).await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 1);
		assert!(svc.verify_password(&user.username, &password).await?);
		// Ensure something else isn't accepted
		assert!(!svc.verify_password(&user.username, "AAAAAAAA").await?);
		// Ensure non-unique labels are rejected for the same user
		svc.new_password(&user, "longiflorum", None).await.unwrap_err();
		// Generate another password and check if it works
		let another_password = svc.new_password(&user, "primrose", None).await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 2);
		assert!(svc.verify_password(&user.username, &another_password).await?);
		// Check that the older password still works
		assert!(svc.verify_password(&user.username, &password).await?);
		// Remove a password
		svc.rm_password_for(&user, "longiflorum").await?;
		assert_eq!(svc.list_passwords_for(&user).await?.len(), 1);
		assert!(!svc.verify_password(&user.username, &password).await?);
		assert!(svc.verify_password(&user.username, &another_password).await?);

		Ok(())
	}

	#[sqlx::test]
	async fn test_login_allowed(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);

		// Create a user
		let uuid = svc.create_user("vsh", None).await?;
		let user = svc.find_user_by_name("vsh").await?.unwrap();
		assert_eq!(user.id, uuid);
		// Create a password for them
		let password = svc.new_password(&user, "longiflorum", None).await?;
		// Check that they can log in
		assert!(svc.verify_password("vsh", &password).await?);
		// Disallow this user to log in
		svc.set_user_login_allowed("vsh", false).await?;
		// Check they can't log in
		assert!(!svc.verify_password("vsh", &password).await?);

		Ok(())
	}

	#[sqlx::test]
	async fn test_non_existent_user(pool: sqlx::PgPool) -> sqlx::Result<()> {
		let svc = create_service(pool);
		assert!(svc.find_user_by_name("vsh").await.unwrap().is_none());

		Ok(())
	}
}
