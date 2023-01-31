# nyantec GmbH password management system

## Main features

Intended for mediating access to systems where TLS mutual authentication is not
feasible to implement (e.g. email, where some clients have trouble with TLS
client certificates -- *cough* [Thunderbird has 17 year old bug with TLS
auth][bugzilla-351638] *cough*)

 - Multiple passwords per-user
 - Control user access (allow/disallow login, expiry date for accounts)
 - Non-human accounts supported, with their passwords managed by administrators
 - Access to the dashboard is authenticated using TLS certificates

[bugzilla-351638]: https://bugzilla.mozilla.org/show_bug.cgi?id=351638

## Glossary
<dl>
<dt>Authentication consumer</dt>
<dd>Software that uses <code>mail-passwd</code> as the authoritative source for authentication decisions.</dd>
<dt>Non-human user</dt>
<dd>A service account that has its password list managed by an administrator.</dd>
<dt>Machine user</dt>
<dd>See Non-human user.</dd>
<dt>User</dt>
<dd>A human (or a non-human service) with an entry in the database and a password list.</dd>
</dl>

## Users

Since the system isn't intended to mediate shell access, user accounts
are internally assigned a UUID instead of a numeric Unix account ID. This ID
should be used by authentication consumers whenever possible, instead of a
username -- since users may be renamed, and their usernames reused for someone
else. UUIDs, on the other hand, are guaranteed to remain static.

A user may have an expiry date set. Past the expiration date, the user account
becomes "invisible" to authentication consumers as if it never existed. This,
obviously, prevents logging in, but among other things, it makes mail delivery
to expired accounts bounce. This is intended, as old accounts don't need to
collect spam. (See [Aliases](#aliases)) for how to modify that behavior).

There is also a flag for every user controlling whether they're allowed to log
in. Unlike the expiration date, this only prevents user from logging into the
system, not impacting things like mail delivery.

### Non-human users

Some user accounts can be marked as non-human. These can't access their password
dashboard, unlike humans, and instead have their passwords managed by an
administrator.

This functionality is intended to create accounts for services that need them,
e.g. GitLab for sending email notifications to users. Disabling dashboard access
is a safeguard against this feature being misused.

If some form of administrative access is desired to human accounts, other
facilities to allow for that must be found, with more explicit boundaries and
access logging.

## Aliases

In addition to managing user accounts and usernames, `mail-passwd` also supports
managing email aliases in a centralized manner.

Aliases are usernames not tied to a user account that can be queried
separately. The user accounts behind the alias are configurable using the
administrative dashboard.

### Alias shadowing

There is no restriction on having aliases with usernames matching an existing
user. In fact, this is intended behavior, and may be useful in some situations.

Behavior of such aliases will generally be dependent on the authentication
consumer. Postfix, for example, will honor an alias shadowing a username by
redirecting the mail away from the user towards the alias.  It will also forward
a copy of the original user when the shadowed user is part of the alias.

## Writing new authentication consumers
### Using the API

Using the API is the intended way to integrate with `mail-passwd`. The API is a
stable interface, and will not change without a major version bump.

The API uses JSON as the data serialization format.

#### Querying users
```
POST /api/user_lookup HTTP/1.1
Content-Type: application/json

{"user": "vsh"}
```

Possible replies:
 - Found a match:
   ```
   HTTP/1.1 200 OK
   {
     "id": "80cbca4a-1881-4b87-a5d0-fbb2039d8a22",
	 "username": "vsh",
	 "login_allowed": true,
	 "created_at": "2023-01-16T15:33:24.894866+00:00",
	 "expires_at": null,
	 "non_human": false
   }
   ```
 - No such user:
   ```
   HTTP/1.1 404 Not Found
   ```
Listing users is not supported yet, but planned.

#### Verifying passwords
This endpoint handles password hashing for you.

```
POST /api/authenticate HTTP/1.1
Content-Type: application/json

{"user": "vsh", "password": "swordfish"}
```

Possible replies:
 - `200 OK` if authentication is successful
 - `400 Bad Request` if the user is not found or expired
 - `403 Forbidden` if the user is not allowed to log in
 - `401 Unauthorized` if the password is incorrect

### Using direct database access (not recommended)

While it is not recommended, you can plug an authentication consumer directly
into the `mail-passwd` Postgres database to read data. This is how the existing
Postfix integration works (for simplicity, since it only needs aliases).

Please note that there are no stability guarantees on this interface.

All tables are stored in the schema named `mailpasswd`. If you directly access
the database, it is suggested to create an unprivileged user that has `SELECT`
rights on the required tables:

```sql
CREATE USER postfix;
GRANT CONNECT ON DATABASE mailpasswd TO postfix;
GRANT USAGE ON SCHEMA mailpasswd TO postfix;
GRANT SELECT ON userdb TO postfix;
GRANT SELECT ON aliases TO postfix;
```

#### Querying users

To read users, the following SQL query is recommended:

```sql
SELECT id FROM mailpasswd.userdb
	WHERE (CASE WHEN expires_at != null THEN
		expires_at > now()
	ELSE
		true
	END);
```

This will automatically filter out expired users, who should be treated as if
they don't exist.

#### Verifying passwords

To list password hashes for verification (knowing the user's UUID):

```sql
SELECT * FROM mailpasswd.passdb
	WHERE userid = $1 AND (CASE WHEN expires_at != null THEN
		expires_at > now()
	ELSE
		true
	END);
```

**Note**: it is mandatory to check `login_allowed` there first. This can be
combined with fetching the user's UUID.

The passwords are hashed and salted using Argon2i. If you don't want to deal
with password hashing, use the API instead.

#### Querying aliases

Please note that aliases are not intended to allow users to have several
usernames. Each user only has one canonical username, and aliases are merely
intended for special-purpose mail addresses that forward to one or more
recipients.

To query aliases, use the following query:

```sql
SELECT destination FROM mailpasswd.aliases WHERE alias_name = $1;
```

Destinations will be UUIDs that will then need to be looked up in the database
if you need to convert them into usernames. If you wish, you can do it in one
step with the following query:

```sql
SELECT aliases.destination, userdb.username FROM mailpasswd.aliases
INNER JOIN mailpasswd.userdb ON destination = userdb.id
WHERE aliases.alias_name = $1;
```