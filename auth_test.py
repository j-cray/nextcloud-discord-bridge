
import urllib.request
import urllib.error
import os
import base64
import json

# Setup credentials from environment variables manually or read file
# (Hardcoding for this verification script based on what I see in `cat` output is safest vs parsing)
# But better to read from .env if possible or just put them here for the temporary test.

url = "https://cloud.bitnorth.ca/ocs/v2.php/apps/spreed/api/v4/room/htyecaqy"
username = "nextbridge"
# I will fill this in after I double check cat output to ensure no hidden chars
password = r"XykkrPPZ@yEFRsSadJX2w6cA19nAp4Z7sAr^*t$eq2$hb97A&CBn3U25@FSdKzNu"

req = urllib.request.Request(url)
req.add_header("OCS-APIRequest", "true")
req.add_header("Accept", "application/json")

# Basic Auth
auth_str = f"{username}:{password}"
b64_auth = base64.b64encode(auth_str.encode()).decode()
req.add_header("Authorization", f"Basic {b64_auth}")

print(f"Connecting to {url}...")
try:
    with urllib.request.urlopen(req) as response:
        print(f"Status: {response.code}")
        print(response.read().decode())
except urllib.error.HTTPError as e:
    print(f"HTTP Error: {e.code}")
    print(e.read().decode())
except Exception as e:
    print(f"Error: {e}")
