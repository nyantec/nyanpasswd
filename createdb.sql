CREATE DATABASE mail TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';
\connect mail
CREATE TABLE userdb (
	   id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
	   username VARCHAR(64) NOT NULL CHECK (username != '') UNIQUE,
	   active BOOLEAN DEFAULT true,
	   created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
	   expires_at TIMESTAMPTZ,
	   CHECK (expires_at IS NULL OR expires_at > created_at)
);
CREATE TABLE passdb (
	   userid UUID NOT NULL REFERENCES userdb(id),
	   label VARCHAR(64) NOT NULL CHECK (label != ''),
	   hash TEXT NOT NULL,
	   created_at TIMESTAMPTZ NOT NULL,
	   expires_at TIMESTAMPTZ,
	   UNIQUE(userid, label)
);

INSERT INTO userdb (username, created_at) VALUES (
	   'vsh',
	   '2022-08-17T14:00:00+0300'
);

INSERT INTO passdb (userid, label, hash, created_at) VALUES (
	   (SELECT id FROM userdb WHERE username = 'vsh'),
	   'test-passwd',
	   '$argon2id$v=19$m=4096,t=3,p=1$CttoKsthTpFkTR9iMq0FzA$dwKsQFnDYPBWrlVExSexLl4zOIguwtjnBLqXVitZAco',
	   '2022-08-17T14:00:00+0300'
);
