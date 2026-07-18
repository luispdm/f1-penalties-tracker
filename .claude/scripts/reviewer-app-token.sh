#!/usr/bin/env bash
# Mints a short-lived GitHub App installation access token for the reviewer bot.
# Reads config from .claude/reviewer-app.env (gitignored). Prints ONLY the token to stdout.
# Never prints the private key or the JWT. Do not commit reviewer-app.env or the private key.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
env_file="${here}/../reviewer-app.env"
# shellcheck disable=SC1090
[ -f "$env_file" ] && { set -a; . "$env_file"; set +a; }

: "${GH_APP_ID:?set GH_APP_ID in .claude/reviewer-app.env}"
: "${GH_APP_PRIVATE_KEY:?set GH_APP_PRIVATE_KEY (path to the PEM) in .claude/reviewer-app.env}"
[ -r "$GH_APP_PRIVATE_KEY" ] || { echo "reviewer-app-token: private key not readable: $GH_APP_PRIVATE_KEY" >&2; exit 1; }

owner="${GH_APP_OWNER:-luispdm}"
repo="${GH_APP_REPO:-f1-penalties-tracker}"
api="${GH_API_URL:-https://api.github.com}"

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

now="$(date +%s)"
iat="$((now - 60))"      # backdate 60s for clock skew
exp="$((now + 540))"     # 9 min; GitHub caps JWT life at 10 min
header="$(printf '{"typ":"JWT","alg":"RS256"}' | b64url)"
payload="$(printf '{"iat":%s,"exp":%s,"iss":"%s"}' "$iat" "$exp" "$GH_APP_ID" | b64url)"
unsigned="${header}.${payload}"
sig="$(printf '%s' "$unsigned" | openssl dgst -sha256 -sign "$GH_APP_PRIVATE_KEY" -binary | b64url)"
jwt="${unsigned}.${sig}"

inst_id="$(curl -fsS -H "Authorization: Bearer ${jwt}" -H "Accept: application/vnd.github+json" \
  "${api}/repos/${owner}/${repo}/installation" | jq -r '.id')"
[ -n "$inst_id" ] && [ "$inst_id" != "null" ] \
  || { echo "reviewer-app-token: app not installed on ${owner}/${repo}" >&2; exit 1; }

curl -fsS -X POST -H "Authorization: Bearer ${jwt}" -H "Accept: application/vnd.github+json" \
  "${api}/app/installations/${inst_id}/access_tokens" | jq -r '.token'
