# Note: Use the following to quickly jump to the juicy parts:
#
# ```python
# serial_stdout_off(); server.wait_for_unit("default.target"); serial_stdout_on()
# exec("\n".join(driver.tests.splitlines()[0:40]))
# ```
self: pkgs:
{ lib, nodes, ... }: let
  test-email = pkgs.writeText "test-email" ''
    From: root@localhost
    To: vsh@localhost
    Subject: test email to user

    This is a test email to a user.
  '';
  test-email-to-alias = pkgs.writeText "test-email" ''
    From: root@localhost
    To: ops@localhost
    Subject: test email to alias

    This is a test email to an alias that will get forwarded.
  '';
in {
  name = "nixos-nyanpasswd";
  hostPkgs = pkgs;

  nodes = {
    server = { config, pkgs, lib, ... }: {
      imports = [ self.nixosModules.default ];

      services.nyanpasswd = {
        enable = true;
        domain = "localhost";
        # Here, we shim certificate validation since we won't use nginx
        rootCACertificate = "";
        crlFile = "";
        dovecot2.enable = true;
        postfix.enable = true;
        adminUids = ["mvs"];
      };

      services.dovecot2 = {
        enable = true;
        enableLmtp = true;
        createMailUser = true;
        mailUser = "vmail";
        mailGroup = "vmail";
        extraConfig = ''
          log_debug = event=*

          service lmtp {
            unix_listener lmtp {
              group = postfix
              mode = 0600
              user = postfix
            }
          }

          service auth {
            unix_listener auth {
              mode = 0660
              # Assuming the default Postfix user and group
              user = postfix
              group = postfix
            }
          }
        '';
      };

      services.postfix = {
        enable = true;
        hostname = "localhost";
        destination = [];
        config = {
          virtual_transport = "lmtp:unix:/run/dovecot2/lmtp";
          virtual_mailbox_domains = ["localhost"];
          # Dovecot auth
          # TODO(@vsh): consider migrating to the module as an option
          smtpd_sasl_type = "dovecot";
          smtpd_sasl_path = "/run/dovecot2/auth";
          smtpd_sasl_auth_enable = true;
        };
      };

      # We disable nginx since we'll access the service directly
      services.nginx.enable = lib.mkForce false;

      system.extraDependencies = [
        test-email
        test-email-to-alias
      ];
    };
  };

  testScript = ''
    import time
    import json
    server.wait_for_unit("default.target")
    # XXX workaround for flaky test, replace by checking for open port on localhost
    time.sleep(1)    
    vsh = "O = nyantec GmbH, CN = Vika Shleina, GN = Viktoriya, SN = Shleina, pseudonym = Vika, UID = vsh"
    mvs = "O = nyantec GmbH, CN = Mikael Voss, GN = Mikael, SN = Voss, UID = mvs"

    with subtest("Check that user creation works when admin doesn't have an account"):
        server.succeed(f"curl --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {mvs}' http://localhost:3000/admin/create_user -d username=vsh -d expires_at=\"\" -d non_human=false")

    with subtest("Check that passwords can be generated"):
        password = server.succeed(f"curl --silent --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {vsh}' -d label=test -d expires_in=noexpiry http://localhost:3000/create_password | grep -o '<code>[^<]*</code>' | cut -b7- | cut -d'<' -f1").strip()
        print("Generated password:", password)

    with subtest("Check that IMAP authentication works"):
        server.succeed(f"curl -vvvvvvv --no-progress-meter imap://localhost:143/ -u vsh:{password}")

    user_json = json.loads(server.succeed("curl --silent --fail http://localhost:3000/api/user_lookup -H 'Content-Type: application/json' -d '{}'".format(json.dumps({"user": "vsh"}))))

    with subtest("Check that Dovecot delivers incoming mail properly"):
        status = server.succeed(f"curl --no-progress-meter imap://localhost:143 -u vsh:{password} -X 'STATUS INBOX (MESSAGES)'").strip()
        if status != "* STATUS INBOX (MESSAGES 0)":
            raise Exception(f"Mailbox not empty: {status}")
        server.succeed("cat ${test-email} | $(dirname $(readlink -f $(which dovecot)))/../libexec/dovecot/dovecot-lda -d vsh -e")
        status = server.succeed(f"curl --no-progress-meter imap://localhost:143 -u vsh:{password} -X 'STATUS INBOX (MESSAGES)'").strip()
        if status != "* STATUS INBOX (MESSAGES 1)":
            raise Exception(f"Mail wasn't delivered: {status}")
        # Strip carriage returns to aid comparison
        mail = server.succeed(f"curl --no-progress-meter 'imap://localhost:143/INBOX;UID=1' -u vsh:{password}").replace("\r\n", "\n")
        with open("${test-email}") as f:
            template = f.read()
        print("Mail sent:     ", repr(template))
        print("Mail delivered:", repr(mail))
        if mail != template:
            raise Exception("Mail doesn't match what was sent")

        with subtest("Ensure that the mailbox with the corresponding UUID exists in the filesystem"):
            server.succeed(f"ls -d ${nodes.server.services.nyanpasswd.dovecot2.mailhome}/{user_json['id']}/Maildir")

    with subtest("Check that Postfix resolves aliases correctly"):
        server.succeed(f"curl --silent --fail -H 'X-Ssl-Verify: SUCCESS' -H 'X-Ssl-Client-Dn: {mvs}' http://localhost:3000/admin/aliases/ -d alias_name=ops -d destination={user_json['id']}")
        server.succeed("cat ${test-email-to-alias} | sendmail ops")
        # Allow things to settle a bit
        # XXX replace with `wait_until_succeeds`
        time.sleep(5)
        status = server.succeed(f"curl --no-progress-meter imap://localhost:143 -u vsh:{password} -X 'STATUS INBOX (MESSAGES)'").strip()
        if status != "* STATUS INBOX (MESSAGES 2)":
            raise Exception(f"Mail wasn't delivered: {status}")
        mail = server.succeed(f"curl --no-progress-meter 'imap://localhost:143/INBOX;UID=2' -u vsh:{password}").replace("\r\n", "\n")
        if not mail.endswith("This is a test email to an alias that will get forwarded.\n"):
            raise Exception("Mail doesn't match what was sent")
  '';
}
