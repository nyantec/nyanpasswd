self:
{ config, pkgs, lib, options, ... }:
with lib;
let
  cfg = config.services.nyantec-mail-passwd;
in {
  options = {
    services.nyantec-mail-passwd = {
      enable = mkEnableOption "mail-passwd, the password management solution for our mail server";
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
      dovecot2.mailLocation = mkOption {
        type = types.str;
        default = "maildir:/var/spool/mail/";
        example = "maildir:/persist/mail/";
        description = mdDoc ''
          The location to store mail of virtual users managed by mail-passwd in. The trailing slash must be present, as the UUID of the user will be appended to this string.
        '';
      };
      postfix = {
        enable = mkEnableOption "integration with Postfix";
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
      systemd.services.mail-passwd = {
        after = [ "network-online.target" ];
        serviceConfig = {
          ExecStart = "${pkgs.mail-passwd}/bin/mail-passwd";
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
      systemd.services.mail-passwd = {
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
          in endsWith "/" cfg.dovecot2.mailLocation;
          message = "services.mail-passwd.dovecot2.mailLocation must end with a slash";
        }
      ];

      systemd.services.dovecot2 = {
        wants = [ "mail-passwd.service" ];
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
          maildir_location = cfg.dovecot2.mailLocation;
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
        mail-passwd = self.packages.${config.nixpkgs.localSystem.system}.default;
      })];
    })
    (lib.mkIf (cfg.enable && cfg.databaseUri == null) {
      systemd.services.mail-passwd = {
        wants = [ "postgresql.service" ];
        after = [ "postgresql.service" ];
      };
      services.postgresql = {
        enable = true;
        initialScript = pkgs.writeText "initial-script.sql" ''
          CREATE DATABASE mailpasswd TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';
        '';
        ensureUsers = lib.mkMerge [
          {
            name = "mailpasswd";
            ensurePermissions = {
              "DATABASE mailpasswd" = "ALL PRIVILEGES";
            };
          }
          (lib.mkIf cfg.postfix.enable {
            name = "postfix";
            ensurePermissions = {
              # TODO(@vsh): I wonder if there is a way to grant privileges
              # to a table not yet created... This would be more secure
              "DATABASE mailpasswd" = "CONNECT";
            };
          })
        ];
      };
    })
    (lib.mkIf (cfg.enable && cfg.postfix.enable) {
      nixpkgs.overlays = [
        (final: prev: {
          postfix = prev.postfix.override { withPgSQL = true; };
        })
      ];

      services.postfix.config = {
        # Use mail-passwd's database for virtual alias maps
        virtual_alias_maps = "pgsql:${pkgs.writeText "postfix-mail-passwd-aliases.cf" ''
          hosts = postgresql:///mailpasswd?host=/run/postgresql
          dbname = mailpasswd
          query = SELECT userdb.username FROM userdb INNER JOIN aliases ON userdb.id = aliases.destination WHERE alias_name = '%u'
        ''}";
      };
    })
  ];
}
