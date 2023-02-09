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
