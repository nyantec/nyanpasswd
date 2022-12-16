-- This assumes the existence of the following database:
-- CREATE DATABASE mail TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';
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
	   created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
	   expires_at TIMESTAMPTZ,
	   UNIQUE(userid, label),
	   CHECK (expires_at IS NULL OR expires_at > created_at)
);
