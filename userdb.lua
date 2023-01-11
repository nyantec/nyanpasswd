if not string.find(package.path, "@lua_path@") then
   package.path = "@lua_path@;" .. package.path;
end
if not string.find(package.cpath, "@lua_cpath@") then
   package.cpath = "@lua_cpath@;" .. package.cpath;
end

local json = require "rapidjson"
local http_client = dovecot.http.client {
    timeout = 5000;
    max_attempts = 3;
    debug = true;
}

function auth_password_verify(request, password)
   local auth_request = http_client:request {
	  url = "http://127.0.0.1:3000/api/authenticate";
	  method = "POST";
   }
   local req = {
	  user = request.user,
	  password = password
   }
   auth_request:add_header("Content-Type", "application/json")
   auth_request:set_payload(json.encode(req))
   local auth_response = auth_request:submit()
   local resp_status = auth_response:status()

   if resp_status == 200 then
	  print("Got HTTP 200!")
	  return dovecot.auth.PASSDB_RESULT_OK, ""
   elseif resp_status == 400 then
	  return dovecot.auth.PASSDB_RESULT_USER_UNKNOWN, "no such user"
   elseif resp_status == 403 then
	  return dovecot.auth.PASSDB_RESULT_USER_DISABLED, "user login disabled by administrator request"
   elseif resp_status == 401 then
	  return dovecot.auth.PASSDB_RESULT_PASSWORD_MISMATCH, "no (non-expired) password matches provided password"
   elseif resp_status == 500 then
	  return dovecot.auth.PASSDB_RESULT_INTERNAL_FAILURE, auth_response:payload()
   else
	  return dovecot.auth.PASSDB_RESULT_INTERNAL_FAILURE, "service returned " .. resp_status
   end
end

function auth_userdb_lookup(request)
   local lookup_request = http_client:request {
	  url = "http://127.0.0.1:3000/api/user_lookup";
	  -- Note: it would be more idiomatic to use GET here.  However,
	  -- it seems that Dovecot lacks facilities for urlencoding things.
	  -- This is bad, so we use JSON and POST here.
	  method = "POST";
   };
   local req = { user = request.user }
   lookup_request:add_header("Content-Type", "application/json")
   lookup_request:set_payload(json.encode(req))
   local lookup_response = lookup_request:submit()
   local status = lookup_response:status()
   if status == 200 then
	  local user = json.decode(lookup_response:payload())
	  local maildir_location = "@maildir_location@" .. user.id
	  print("Got HTTP 200 with UUID " .. user.id .. ", maildir: " .. maildir_location)
	  return dovecot.auth.USERDB_RESULT_OK, "mail_location=" .. maildir_location
   elseif status == 404 then
	  return dovecot.auth.USERDB_RESULT_USER_UNKNOWN, payload
   else
	  return dovecot.auth.USERDB_RESULT_INTERNAL_FAILURE, payload
   end
end
