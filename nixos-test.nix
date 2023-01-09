self:
{ lib, ... }: {
  name = "nixos-mail-passwd";

  nodes = {
    server = { config, pkgs, lib, ... }: {
      imports = [ self.nixosModules.default ];

      services.nyantec-mail-passwd = {
        enable = true;
        domain = "localhost";
        # Here, we shim certificate validation since we won't use nginx
        rootCACertificate = "";
        crlFile = "";
        dovecot2.enable = true;
        adminUids = ["mvs"];
      };

      services.dovecot2 = {
        enable = true;
      };

      # We disable nginx since we'll access the service directly
      services.nginx.enable = lib.mkForce false;
    };
  };

  testScript = ''
    server.wait_for_unit("default.target")
    vsh = "O = nyantec GmbH, CN = Vika Shleina, GN = Viktoriya, SN = Shleina, pseudonym = Vika, UID = vsh"
    mvs = "O = nyantec GmbH, CN = Mikael Voss, GN = Mikael, SN = Voss, UID = mvs"

    server.succeed(f"curl --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {vsh}' http://localhost:3000/")

    password = server.succeed(f"curl --silent --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {vsh}' -d label=test -d expires_in=noexpiry https://localhost:3000/create_password").strip()

    server.succeed(f"curl --silent --fail imap://localhost:143/ -u vsh:{password}")
  '';
}
