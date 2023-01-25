CREATE TABLE aliases (
	   alias_name VARCHAR(64) NOT NULL CHECK (alias_name != ''),
	   destination UUID NOT NULL REFERENCES userdb(id),
	   UNIQUE(alias_name, destination)
);
