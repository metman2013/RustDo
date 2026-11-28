#!/usr/bin/env bash
# Turns new posts in the RustDo Discord's bug and request channels into GitHub issues
# (labelled "from-discord", plus "bug" or "enhancement"), and replies in Discord with the
# issue link. Run by .github/workflows/discord-intake.yml every hour.
#
# Needs: DISCORD_BOT_TOKEN (secret), DISCORD_BUGS_CHANNEL and DISCORD_REQUESTS_CHANNEL
# (channel ids), GH_TOKEN and GITHUB_REPOSITORY (from Actions). Uses curl, jq and gh.
#
# Messages are written by anyone: they're only ever passed around as data (jq / files),
# never put into a command, and Discord is told not to turn anything into @mentions.
set -euo pipefail

API=https://discord.com/api/v10
# Only recent posts (so the first run doesn't import the channel's whole history, and a
# missed run or two still catches up). Posts already tracked are skipped by their id.
SINCE_DAYS=${SINCE_DAYS:-7}
since=$(date -u -d "-${SINCE_DAYS} days" +%s)

discord() { # method path [json]
  local method=$1 path=$2
  if [ $# -ge 3 ]; then
    curl -fsS -X "$method" -H "Authorization: Bot $DISCORD_BOT_TOKEN" -H "Content-Type: application/json" \
      --data "$3" "$API$path"
  else
    curl -fsS -X "$method" -H "Authorization: Bot $DISCORD_BOT_TOKEN" "$API$path"
  fi
}

gh label create from-discord --color 5865F2 --description "Reported in the RustDo Discord" --force >/dev/null
gh label create bug --color d73a4a --force >/dev/null
gh label create enhancement --color a2eeef --force >/dev/null

# Message ids already turned into issues (a marker in each issue's body).
tracked=$(gh issue list --label from-discord --state all --limit 1000 --json body --jq '.[].body' |
  grep -o 'discord-msg:[0-9]*' | cut -d: -f2 | sort -u || true)

intake() { # channel_id kind label
  local channel=$1 kind=$2 label=$3
  [ -n "$channel" ] || return 0
  local guild
  guild=$(discord GET "/channels/$channel" | jq -r '.guild_id')
  local messages
  messages=$(discord GET "/channels/$channel/messages?limit=50")
  # Oldest first, people only, top-level posts (not replies), with some text or a picture.
  echo "$messages" | jq -c --argjson since "$since" '
    reverse | .[]
    | select(.author.bot != true)
    | select(.type == 0)
    | select((.timestamp | sub("\\.[0-9]+"; "") | sub("\\+00:00$"; "Z") | fromdateiso8601) >= $since)
    | select((.content | length) >= 10 or (.attachments | length) > 0)' |
  while read -r m; do
    local id
    id=$(jq -r '.id' <<<"$m")
    if grep -qx "$id" <<<"$tracked"; then continue; fi
    local link="https://discord.com/channels/$guild/$channel/$id"
    local title body
    title=$(jq -r --arg kind "$kind" '
      (.content | split("\n")[0] | gsub("[\\r\\t]"; " ") | .[0:80]) as $t
      | "[\($kind)] " + (if ($t | length) > 0 then $t else "(picture)" end)' <<<"$m")
    body=$(jq -r --arg link "$link" --arg id "$id" '
      "**From the RustDo Discord**, posted by **" + (.author.global_name // .author.username) + "** ([message](" + $link + ")):\n\n"
      + ((.content // "") | split("\n") | map("> " + .) | join("\n"))
      + (if (.attachments | length) > 0 then "\n\nAttachments:\n" + ([.attachments[] | "- [" + .filename + "](" + .url + ")"] | join("\n")) else "" end)
      + "\n\n<!-- discord-msg:" + $id + " -->"' <<<"$m")
    local tmp
    tmp=$(mktemp)
    printf '%s\n' "$body" >"$tmp"
    local url
    url=$(gh issue create --title "$title" --body-file "$tmp" --label from-discord --label "$label")
    rm -f "$tmp"
    tracked=$(printf '%s\n%s' "$tracked" "$id")
    echo "message $id -> $url"
    # Let them know it was seen (a reply to their post; no mentions).
    local number=${url##*/}
    local reply
    reply=$(jq -n --arg text "Thanks! Tracked as issue #$number: <$url>" --arg id "$id" \
      '{content: $text, message_reference: {message_id: $id, fail_if_not_exists: false}, allowed_mentions: {parse: [], replied_user: false}}')
    discord POST "/channels/$channel/messages" "$reply" >/dev/null || echo "::warning::couldn't reply in Discord to $id"
  done
}

intake "${DISCORD_BUGS_CHANNEL:-}" "Bug" bug
intake "${DISCORD_REQUESTS_CHANNEL:-}" "Request" enhancement
