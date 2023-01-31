# nyantec GmbH password management system

## Main features

Intended for mediating access to systems where TLS mutual authentication is not
feasible to implement (e.g. email, where some clients have trouble with TLS
client certificates -- *cough* Thunderbird has 16 year old bugs related to that
*cough*)

 - Multiple passwords per-user
 - Control user access (allow/disallow login, expiry date for accounts)
 - Non-human accounts supported, with their passwords managed by administrators
 - Access to the dashboard is authenticated using TLS certificates

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