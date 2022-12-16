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
		// SAFETY: the Alphanumeric distribution only produces
		// ASCII alphanumeric characters, which are valid UTF-8
		unsafe { String::from_utf8_unchecked(rng.sample_iter(&Alphanumeric).take(PASSWORD_LENGTH).collect::<Vec<u8>>()) }
	}
}

#[derive(sqlx::FromRow)]
struct Password {
	userid: Uuid,
	label: String,
	hash: String,
	created_at: chrono::DateTime<chrono::FixedOffset>,
	expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(sqlx::FromRow)]
struct User {
	id: Uuid,
	username: String,
	active: bool,
	created_at: chrono::DateTime<chrono::FixedOffset>,
	expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

enum MigrationsDone {}
enum Created {}

mod sealed {
	pub trait InitState {}
	impl InitState for super::MigrationsDone {}
	impl InitState for super::Created {}
}

pub struct Service<S: sealed::InitState> {
	db: sqlx::PgPool,
	argon2: Argon2<'static>,
	_migrations: std::marker::PhantomData<S>,
}

impl Service<Created> {
	fn new(db: sqlx::PgPool) -> Self {
		Self {
			db,
			argon2: argon2::Argon2::new(argon2::Algorithm::Argon2i, Default::default(), Default::default()),
			_migrations: std::marker::PhantomData,
		}
	}

	async fn run_migrations(self) -> sqlx::Result<Service<MigrationsDone>> {
		MIGRATOR.run(&self.db).await?;

		Ok(Service::<MigrationsDone> {
			_migrations: std::marker::PhantomData::<MigrationsDone>,
			db: self.db,
			argon2: self.argon2,
		})
	}
}

impl Service<MigrationsDone> {
	/// Generate a password for a user designated by `user` and save
	/// it with the corresponding `label`.
	async fn new_password(
		&self,
		user: &str,
		label: &str,
		expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
	) -> sqlx::Result<String> {
		let mut rng = rand::rngs::OsRng;
		let password: String = self::util::gen_password(&mut rng);

		sqlx::query(
			"INSERT INTO passdb (userid, label, hash, expires_at) VALUES ((SELECT id FROM userdb WHERE username = $1), $2, $3, $4)",
		)
		.bind(user)
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
	/// Irreversibly remove a password designated by `label` from the
	/// specified user.
	async fn rm_password(&self, user: &str, label: &str) -> sqlx::Result<()> {
		sqlx::query("DELETE FROM passdb WHERE userid = (SELECT id FROM userdb WHERE username = $1) AND label = $2")
			.bind(&user)
			.bind(&label)
			.execute(&self.db)
			.await?;

		Ok(())
	}
	/// List passwords owned by the current user.
	async fn list_passwords(&self, user: &str) -> sqlx::Result<Vec<Password>> {
		sqlx::query_as::<_, Password>("SELECT * FROM passdb WHERE userid = (SELECT id FROM userdb WHERE username = $1)")
			.bind(user)
			.fetch_all(&self.db)
			.await
	}

	/// Verify a password for a user identified by their username.
	async fn verify_password(&self, user: &str, password: &str) -> sqlx::Result<bool> {
		let mut stream = sqlx::query_scalar::<_, String>(
			"SELECT hash FROM passdb WHERE userid = (SELECT userid FROM userdb WHERE username = $1)",
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
	async fn find_user_by_name(&self, username: &str) -> sqlx::Result<User> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb WHERE username = $1")
			.bind(username)
			.fetch_one(&self.db)
			.await
	}
	/// List all users.
	async fn list_users(&self) -> sqlx::Result<Vec<User>> {
		sqlx::query_as::<_, User>("SELECT * FROM userdb").fetch_all(&self.db).await
	}
	/// Create a new user.
	async fn create_user(&self, username: &str, expires_at: Option<chrono::DateTime<chrono::FixedOffset>>) -> sqlx::Result<Uuid> {
		sqlx::query_as::<_, (Uuid,)>(
			"
INSERT INTO userdb (username, expires_at) VALUES ($1, $2) RETURNING id",
		)
		.bind(username)
		.bind(expires_at)
		.fetch_one(&self.db)
		.await
		.map(|ret| ret.0)
	}
	/// Activate or deactivate a user.
	async fn set_user_active(&self, user: &str, active: bool) -> sqlx::Result<()> {
		sqlx::query("UPDATE userdb (active) VALUES (true) WHERE username = $1")
			.bind(user)
			.execute(&self.db)
			.await?;

		Ok(())
	}
}

#[cfg(test)]
mod test {
	#[sqlx::test]
	async fn smoke_test(pool: sqlx::PgPool) -> sqlx::Result<()> {
		// Note: you are DEFINITELY NOT SUPPOSED to be creating this
		// object like that! SQLx test harness automatically applies
		// migrations, so we don't need to run them a second time.
		//
		// Since the typestate objects are actually private, you will
		// not be able to bypass migrations normally.
		let svc = crate::Service::<super::MigrationsDone> {
			_migrations: std::marker::PhantomData::<super::MigrationsDone>,
			db: pool,
			argon2: argon2::Argon2::new(argon2::Algorithm::Argon2i, Default::default(), Default::default()),
		};

		let uuid = svc.create_user("vsh", None).await?;
		let user = svc.find_user_by_name("vsh").await?;
		assert_eq!(user.id, uuid);

		// Check that no passwords are defined
		assert_eq!(svc.list_passwords("vsh").await?.len(), 0);
		assert!(!svc.verify_password("vsh", "AAAAAAAA").await?);
		// Generate a password and ensure it matches
		let password = svc.new_password("vsh", "longiflorum", None).await?;
		assert_eq!(svc.list_passwords("vsh").await?.len(), 1);
		assert!(svc.verify_password("vsh", &password).await?);
		// Ensure something else isn't accepted
		assert!(!svc.verify_password("vsh", "AAAAAAAA").await?);
		// Ensure non-unique labels are rejected for the same user
		svc.new_password("vsh", "longiflorum", None).await.unwrap_err();
		// Generate another password and check if it works
		let another_password = svc.new_password("vsh", "primrose", None).await?;
		assert_eq!(svc.list_passwords("vsh").await?.len(), 2);
		assert!(svc.verify_password("vsh", &another_password).await?);
		// Check that the older password still works
		assert!(svc.verify_password("vsh", &password).await?);
		// Remove a password
		svc.rm_password("vsh", "longiflorum").await?;
		assert_eq!(svc.list_passwords("vsh").await?.len(), 1);
		assert!(!svc.verify_password("vsh", &password).await?);
		assert!(svc.verify_password("vsh", &another_password).await?);

		Ok(())
	}
}
