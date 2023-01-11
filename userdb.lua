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
	  url = "http://localhost:3000/api/authenticate/";
	  method = "POST";
   }
   local req = {
	  user = request.user,
	  password = password
   }
   auth_request:set_payload(json.encode(req))
   local auth_response = auth_request:submit()
   local resp_status = auth_response:status()

   if resp_status == 200 then
	  return dovecot.auth.PASSDB_RESULT_OK, ""
   elseif resp_status == 400 then
	  return dovecot.auth.PASSDB_RESULT_USER_UNKNOWN, "no such user"
   elseif resp_status == 403 then
	  return dovecot.status.PASSDB_RESULT_USER_DISABLED, "user login disabled by administrator request"
   elseif resp_status == 401 then
	  local payload = auth_response:payload()
	  if payload == "expired" then
		 return dovecot.status.PASSDB_RESULT_PASS_EXPIRED, "password expired"
	  else
		 return dovecot.status.PASSDB_RESULT_PASSWORD_MISMATCH, "no password matches provided password"
	  end
   elseif resp_status == 500 then
	  return dovecot.status.PASSDB_RESULT_INTERNAL_FAILURE, auth_response:payload()
   else
	  return dovecot.status.PASSDB_RESULT_INTERNAL_FAILURE, "service returned " .. resp_status
   end
end

function auth_userdb_lookup(request)
   local lookup_request = http_client:request {
	  url = "http://localhost:3000/api/user_lookup/";
	  -- Note: it would be more idiomatic to use GET here.  However,
	  -- it seems that Dovecot lacks facilities for urlencoding things.
	  -- This is bad, so we use JSON and POST here.
	  method = "POST";
   };
   local req = { user = request.user }
   lookup_request:set_payload(json.encode(req))
   local lookup_response = lookup_request:submit()
   local status = lookup_response:status()
   local payload = lookup_request:payload()
   if status == 200 then
	  return dovecot.status.USERDB_RESULT_OK, json.decode(payload)
   elseif status == 404 then
	  return dovecot.status.USERDB_RESULT_USER_UNKNOWN, payload
   else
	  return dovecot.status.USERDB_RESULT_INTERNAL_FAILURE, payload
   end
end
