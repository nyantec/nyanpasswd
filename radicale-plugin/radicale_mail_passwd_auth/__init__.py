# Copyright © 2022 nyantec GmbH <oss@nyantec.com>
# Written by Vika Shleina <vsh@nyantec.com>
#
# Provided that these terms and disclaimer and all copyright notices
# are retained or reproduced in an accompanying document, permission
# is granted to deal in this work without restriction, including un‐
# limited rights to use, publicly perform, distribute, sell, modify,
# merge, give away, or sublicence.
#
# This work is provided "AS IS" and WITHOUT WARRANTY of any kind, to
# the utmost extent permitted by applicable law, neither express nor
# implied; without malicious intent or gross negligence. In no event
# may a licensor, author or contributor be held liable for indirect,
# direct, other damage, loss, or other issues arising in any way out
# of dealing in the work, even if advised of the possibility of such
# damage or existence of a defect, except proven that it results out
# of said person's immediate fault when using the work as intended.
import requests
from radicale.auth import BaseAuth
from radicale.log import logger

PLUGIN_CONFIG_SCHEMA = {
    "auth": {
        "mail_passwd_uri": {
            "value": "",
            "type": str
        }
    }
}


class Auth(BaseAuth):
    def __init__(self, configuration):
        super().__init__(configuration.copy(PLUGIN_CONFIG_SCHEMA))

    def login(self, login, password):
        # Get password from configuration option
        uri = self.configuration.get("auth", "mail_passwd_uri")
        # Check authentication
        logger.debug("Login attempt by %r with password %s", login, password)

        # Note: some applications try to use the email address as the login.
        # Mail-passwd is designed to manage unlimited domains with the same users,
        # so we shall strip the domain.
        if "@" in login:
            login = login.split("@")[0]
            if login == "":
                return ""

        response = requests.post(
            uri + "/api/authenticate",
            json = { "user": login, "password": password },
            headers = {
                "Content-Type": "application/json",
            }
        )
        logger.debug("mail-passwd responded with %s: %s", response.status_code, response)
        if response.status_code == 200:
            userdata_response = requests.post(
                uri + "/api/user_lookup",
                json = { "user": login },
                headers = {
                    "Content-Type": "application/json",
                    "Accept": "application/json"
                }
            )
            userdata = userdata_response.json()
            logger.debug("Login attempt successful, looked up userdata: %s", userdata),
            # Return the UUID instead of the username
            return userdata["id"]
        else:
            return ""
