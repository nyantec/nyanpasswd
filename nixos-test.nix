# Note: Use the following to quickly jump to the juicy parts:
#
# ```python
# serial_stdout_off()
# server.wait_for_unit("default.target")
# serial_stdout_on()
# exec("\n".join(driver.tests.splitlines()[0:13]))
# ```
self: pkgs:
{ lib, ... }: {
  name = "nixos-mail-passwd";
  hostPkgs = pkgs;

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
        extraConfig = ''
          log_debug = event=*
        '';
        createMailUser = true;
        mailUser = "vmail";
        mailGroup = "vmail";
      };

      # We disable nginx since we'll access the service directly
      services.nginx.enable = lib.mkForce false;
    };
  };

  testScript = ''
    import time
    server.wait_for_unit("default.target")
    # XXX workaround for flaky test, replace by checking for open port on localhost
    time.sleep(1)    
    vsh = "O = nyantec GmbH, CN = Vika Shleina, GN = Viktoriya, SN = Shleina, pseudonym = Vika, UID = vsh"
    mvs = "O = nyantec GmbH, CN = Mikael Voss, GN = Mikael, SN = Voss, UID = mvs"

    with subtest("Check that user creation works when admin doesn't have an account"):
        server.succeed(f"curl --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {mvs}' http://localhost:3000/admin/create_user -d username=vsh -d expires_at=\"\"")

    with subtest("Check that passwords can be generated"):
        password = server.succeed(f"curl --silent --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {vsh}' -d label=test -d expires_in=noexpiry http://localhost:3000/create_password | grep -o '<code>[^<]*</code>' | cut -b7- | cut -d'<' -f1").strip()
        print("Generated password:", password)

    with subtest("Check that IMAP authentication works:"):
        server.succeed(f"curl -vvvvvvv --no-progress-meter imap://localhost:143/ -u vsh:{password}")
  '';
}
