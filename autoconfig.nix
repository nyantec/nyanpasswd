{ config, options, pkgs, lib, ... }:
with lib;
let
  cfg = config.services.nyanpasswd.autoconfig;

  serverOptions = {
    hostname = mkOption {
      type = types.str;
      default = "${config.services.nyantec-mail-passwd.domain}";
      defaultText = "\${config.services.nyantec-mail-passwd.domain}";
      description = ''
        The hostname for the server that will accept incoming mail.
      '';
    };
    port = default: example: mkOption {
      type = types.port;
      default = default;
      example = 587;
      description = ''
        The port at which the service is exposed. This may be dependent on `socketType`.
      '';
    };
    socketType = mkOption {
      type = types.enum ["plain" "SSL" "STARTTLS"];
      default = "SSL";
      example = "STARTTLS";
      description = mdDoc ''
        Whether encryption should be enabled:
        - `plain` - no encryption (not recommended)
        - `SSL` - mandatory encryption (usually on a different port)
        - `STARTTLS` - opportunistic encryption
      '';
    };
    authentication = mkOption {
      type = types.listOf (types.enum [
        "password-cleartext" "password-encrypted"
        "NTLM" "GSSAPI"
        "client-IP-address" "TLS-client-cert" "OAuth2"
        "none"
      ]);
      default = ["password-cleartext"];
      description = mdDoc ''
        Which forms of authentication are recommended. Available options:
        - `password-cleartext` - send password in the clear (PLAIN/LOGIN)
        - `password-encrypted` - send password encrypted with CRAM-MD5/DIGEST-MD5
        - `NTLM` - use Windows' NTLM login mechanism
        - `GSSAPI` - use Kerberos for SSO
        - `client-IP-address` - no authentication, the server recognizes a user based on the IP address.
        - `TLS-client-cert` - request a TLS client certificate. (Not supported by Thunderbird)
        - `OAuth2` - isn't supported in most clients, see [Mozilla wiki][1] for more info.
        - `none` - no authentication at all.

        [1]: https://wiki.mozilla.org/Thunderbird:Autoconfiguration:ConfigFileFormat#OAuth2
      '';
    };
  };
    
  autoconfig = spec: ''
    <?xml version="1.0"?>
    <clientConfig version="1.1">
      <emailProvider id="example.com">
        ${concatStringsSep "" (map (domain: "<domain>${domain}</domain>") spec.domains)}

        <displayName>${spec.displayName}</displayName>
        <displayShortName>${spec.displayShortName}</displayShortName>

        <incomingServer type="imap">
          <hostname>${spec.incomingServer.hostname}</hostname>
          <port>${builtins.toString spec.incomingServer.port}</port>
          <socketType>${spec.incomingServer.socketType}</socketType>
          <username>%EMAILLOCALPART%</username>
          ${concatStringsSep "" (map (auth: "<authentication>${auth}</authentication>") spec.incomingServer.authentication)}
        </incomingServer>

        <outgoingServer type="smtp">
          <hostname>${spec.outgoingServer.hostname}</hostname>
          <port>${builtins.toString spec.outgoingServer.port}</port>
          <socketType>${spec.outgoingServer.socketType}</socketType>
          <username>%EMAILLOCALPART%</username>
          ${concatStringsSep "" (map (auth: "<authentication>${auth}</authentication>") spec.outgoingServer.authentication)}
          <addThisServer>true</addThisServer>
          <!-- <useGlobalPreferredServer>true</useGlobalPreferredServer> -->
        </outgoingServer>

        <!-- Not yet implemented, see bug 586364. -->
        <enable visiturl="https://${config.services.nyantec-mail-passwd.domain}/">
          <instruction>Create a password for this device using the dashboard</instruction>
          <instruction lang="de">Erstellen Sie ein Passwort für dieses Gerät mit dem Dashboard</instruction>
        </enable>
      </emailProvider>
    </clientConfig>
  '';
  generateAutoconfig = domain: {
    name = "autoconfig.${domain}";
    value = {
      forceSSL = true;
      locations = {
        "=/mail/config-v1.1.xml" = {
          alias = pkgs.writeText "autoconfig.xml" (autoconfig cfg);
        };
      };
    };
  };
in {
  options.services.nyanpasswd = {
    # https://wiki.mozilla.org/Thunderbird:Autoconfiguration:ConfigFileFormat
    autoconfig = {
      enable = mkEnableOption "Thunderbird autoconfig file for the mail";
      domains = mkOption {
        type = types.listOf types.str;
        example = ["nyantec.com"];
        description = mdDoc ''
          A list of domains for which autoconfig files should be exposed.
        '';
      };
      displayName = mkOption {
        type = types.str;
        example = "nyantec GmbH mail";
        description = mdDoc ''
          The user-visible name for this mail server.
        '';
      };
      displayShortName = mkOption {
        type = types.str;
        example = "nyantec";
        description = mdDc ''
          A very short name for this mail server.
        '';
      };
      incomingServer = mkOption {
        type = types.submodule {
          options = {
            hostname = serverOptions.hostname;
            port = serverOptions.port 993 143;
            socketType = serverOptions.socketType;
            authentication = serverOptions.authentication;
          };
        };
        default = {};
      };
      outgoingServer = mkOption {
        type = types.submodule {
          options = {
            hostname = serverOptions.hostname;
            port = serverOptions.port 465 587;
            socketType = serverOptions.socketType;
            authentication = serverOptions.authentication;
          };
        };
        default = {};
      };
    };
  };
  config = mkIf cfg.enable {
    services.nginx.virtualHosts = listToAttrs (map generateAutoconfig cfg.domains);
  };
}
