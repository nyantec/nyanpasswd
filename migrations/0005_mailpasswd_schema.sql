CREATE SCHEMA IF NOT EXISTS mailpasswd;
ALTER TABLE userdb SET SCHEMA mailpasswd;
ALTER TABLE passdb SET SCHEMA mailpasswd;
ALTER TABLE aliases SET SCHEMA mailpasswd;
