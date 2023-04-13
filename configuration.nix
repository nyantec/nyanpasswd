self:
{ config, pkgs, lib, options, ... }:
with lib;
let
  cfg = config.services.nyanpasswd;
in {
  imports = [
    ./autoconfig.nix
    (mkRemovedOptionModule [ "services" "nyanpasswd" "dovecot2" "mailLocation" ] ''
      Use `services.nyanpasswd.dovecot2.mailhome` instead. Additionally, run the following command to migrate old mailboxes:

      ```
      mkdir $mailhome
      for maildir in $mailLocation/*; do
          mkdir -p "$mailhome/$(basename "$maildir")"
          mv -T "$maildir" "$mailhome/$(basename "$maildir")/Maildir"
      end
      ```
    '')
    (mkRenamedOptionModule ["services" "nyantec-mail-passwd"] ["services" "nyanpasswd"])
  ];
  options = {
    services.nyanpasswd = {
      enable = mkEnableOption "nyanpasswd, the password management solution for our mail server";
      databaseUri = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "postgres://localhost?dbname=mail";
        description = mdDoc ''
          The database connection string to be used. If `null`,
          postgres is automatically configured.

          Note: MySQL is not supported due to quirks in the database
          queries. Postgres is heavily recommended.
        '';
      };
      domain = mkOption {
        type = types.str;
        default = "localhost";
        description = mdDoc ''
          nginx vhost on which to deploy the service.

          You can add additional configuration to it later. TLS is
          mandatory. TLS client certificate validation is
          automatically configured.
        '';
      };
      rootCACertificate = mkOption {
        type = types.either types.str types.path;
        example = "/var/lib/nyantec-crl/nyantec_Root_CA.pem";
        description = mdDoc ''
          TLS root CA certificate against which client certificates
          are validated.
        '';
      };
      crlFile = mkOption {
        type = types.either types.str types.path;
        example = "/var/lib/nyantec-crl/nyantec-combined.pem";
        description = mdDoc ''
          A certificate revocation list against which certificates are
          validated.
        '';
      };
      adminUids = mkOption {
        type = types.listOf types.str;
        default = [];
        example = ["mvs" "mak" "vsh"];
        description = mdDoc ''
          A list of UIDs that will be granted access to the
          administrative functions.
        '';
      };
      user = mkOption {
        type = types.nullOr types.str;
        example = "mailpasswd";
        default = null;
        description = mdDoc ''
          The user ID under which mail-passwd will be running. Leave as `null`
          to autoconfigure.
        '';
      };
      dovecot2.enable = mkEnableOption "integration with Dovecot";
      dovecot2.mailhome = mkOption {
        type = types.str;
        default = "/var/vmail";
        example = "/persist/vmail";
        description = mdDoc ''
          The location to store mail data of virtual users managed by nyanpasswd in.
        '';
      };
      postfix = {
        enable = mkEnableOption "integration with Postfix";
      };
      radicale = {
        enable = mkEnableOption "integration with Radicale";
      };
    };
  };

  config = lib.mkMerge [
    (lib.mkIf cfg.enable {
      services.nginx.enable = true;
      services.nginx.virtualHosts."${cfg.domain}" = {
        forceSSL = true;
        locations."/" = {
          proxyPass = "http://localhost:3000/";
          extraConfig = ''
            proxy_set_header X-SSL-Verify $ssl_client_verify;
            proxy_set_header X-SSL-Client-Dn $ssl_client_s_dn;
          '';
        };
        locations."/api" = {
          extraConfig = ''
            return 403;
          '';
        };
        extraConfig = ''
          ssl_verify_client on;
          ssl_client_certificate ${cfg.rootCACertificate};
          ssl_crl ${cfg.crlFile};
        '';
      };
      nixpkgs.overlays = [(final: prev: {
        mail-passwd = lib.warn "mail-passwd was renamed to nyanpasswd." final.nyanpasswd;
        nyanpasswd = self.packages.${config.nixpkgs.localSystem.system}.default;
      })];
      systemd.services.nyanpasswd = {
        after = [ "network-online.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.nyanpasswd}/bin/nyanpasswd";
          User = lib.mkIf (cfg.user != null) cfg.user;
        };
        environment = {
          DATABASE_URL = if (cfg.databaseUri == null)
                         then
                           "postgres://localhost?dbname=mailpasswd&host=/run/postgresql"
                         else
                           cfg.databaseUri;
          ADMIN_UIDS = lib.concatStringsSep " " cfg.adminUids;
        };
      };
    })
    (lib.mkIf (cfg.enable && cfg.user == null) {
      users.users.mailpasswd = {
        isSystemUser = true;
        group = "mailpasswd";
      };
      users.groups.mailpasswd = {};
      systemd.services.nyanpasswd = {
        serviceConfig.User = "mailpasswd";
      };
    })
    (lib.mkIf (cfg.enable && cfg.dovecot2.enable) {
      assertions = [
        {
          assertion = let
            getLastChar = s: lib.strings.substring
              ((lib.strings.stringLength s)-1)
              ((lib.strings.stringLength s)-1)
              s;
            endsWith = c: s: (getLastChar s) == c;
          in !(endsWith "/" cfg.dovecot2.mailhome);
          message = "services.nyanpasswd.dovecot2.mailhome must not end with a slash";
        }
      ];

      systemd.services.dovecot2 = {
        wants = [ "nyanpasswd.service" ];
      };

      services.dovecot2.extraConfig = let
        makeLuaPath = subDir: paths: concatStringsSep ";" (map (path: path + "/" + subDir) (filter (x: x != null) paths));
        packages = with pkgs.lua53Packages; [
          rapidjson
        ];
        luaPath = (makeLuaPath "lib/lua/5.3/?.lua" packages);
        luaCPath = (makeLuaPath "lib/lua/5.3/?.so" packages);
        userdb = pkgs.substituteAll {
          src = ./userdb.lua;
          lua_path = luaPath;
          lua_cpath = luaCPath;
          mailhome = cfg.dovecot2.mailhome;
        };
      in ''
        userdb {
          driver = lua
          args = file=${userdb}
        }

        passdb {
          driver = lua
          args = file=${userdb}
        }
      '';
      # they removed `services.dovecot2.package`...
      nixpkgs.overlays = [(final: prev: {
        dovecot = prev.dovecot.override { withLua = true; };
      })];
      systemd.tmpfiles.rules = [
        "d ${cfg.dovecot2.mailhome} 0770 ${config.services.dovecot2.mailUser} ${config.services.dovecot2.mailGroup} -"
      ];
    })
    (lib.mkIf (cfg.enable && cfg.databaseUri == null) {
      systemd.services.nyanpasswd = {
        wants = [ "postgresql.service" ];
        after = [ "postgresql.service" ];
      };
      services.postgresql = {
        enable = true;
        initialScript = pkgs.writeText "initial-script.sql" ''
          CREATE DATABASE mailpasswd TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';
          \connect mailpasswd
          CREATE SCHEMA IF NOT EXISTS mailpasswd;
          CREATE USER mailpasswd;
          ALTER SCHEMA mailpasswd OWNER TO mailpasswd;
          GRANT ALL PRIVILEGES ON DATABASE mailpasswd TO mailpasswd;
          ${lib.optionalString cfg.postfix.enable ''
            CREATE USER postfix;
            GRANT CONNECT ON DATABASE mailpasswd TO postfix;
            GRANT USAGE ON SCHEMA mailpasswd TO postfix;
            SET ROLE mailpasswd;
            ALTER DEFAULT PRIVILEGES FOR USER mailpasswd IN SCHEMA mailpasswd GRANT SELECT ON TABLES TO postfix;
            RESET ROLE;
          ''}
        '';
      };
    })
    (lib.mkIf (cfg.enable && cfg.postfix.enable) {
      nixpkgs.overlays = [
        (final: prev: {
          postfix = prev.postfix.override { withPgSQL = true; };
        })
      ];
      services.postfix.config = {
        # Use nyanpasswd's database for virtual alias maps
        virtual_alias_maps = "pgsql:${pkgs.writeText "postfix-nyanpasswd-aliases.cf" ''
          hosts = postgresql:///mailpasswd?host=/run/postgresql
          dbname = mailpasswd
          query = SELECT userdb.username FROM mailpasswd.userdb INNER JOIN mailpasswd.aliases ON userdb.id = aliases.destination WHERE alias_name = '%u'
        ''}";
      };
    })
    (lib.mkIf (cfg.enable && cfg.radicale.enable) {
      services.radicale = {
        enable = true;
        package = pkgs.radicale.overrideAttrs (old: {
          propagatedBuildInputs = old.propagatedBuildInputs ++ [
            self.packages.${config.nixpkgs.localSystem.system}.radicale-plugin-nyanpasswd
          ];
        });
        settings = {
          auth = {
            type = "radicale_mail_passwd_auth";
            mail_passwd_uri = "http://localhost:3000";
          };
        };
      };
    })
  ];
}
