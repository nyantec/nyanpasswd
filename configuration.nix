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
          ADMIN_UIDS = lib.concatSepStrings " " cfg.adminUids;
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
      systemd.services.dovecot2 = {
        wants = [ "mail-passwd.service" ];
      };

      services.dovecot2.extraConfig = ''
        userdb {
          driver = lua
          args = file=${./userdb.lua}
        }

        passdb {
          driver = lua
          args = file=${./userdb.lua}
        }
      '';
      # they removed `services.dovecot2.package`...
      nixpkgs.overlays = [(final: prev: {
        dovecot = prev.dovecot.override { withLua = true; };
        mail-passwd = self.packages.${config.nixpkgs.localSystem.system}.default;
      })];
      systemd.services.dovecot2 = {
        environment.LUA_PATH = lib.makeSearchPath "/lib/lua/5.3" (with pkgs.lua53Packages; [
          rapidjson
        ]);
      };
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
        ensureUsers = [
          {
            name = "mailpasswd";
            ensurePermissions = {
              "DATABASE mailpasswd" = "ALL PRIVILEGES";
            };
          }
        ];
      };
    })
  ];
}
