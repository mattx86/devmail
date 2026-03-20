#!/usr/bin/env bash
set -euo pipefail

# Start devmail in the background, forwarding all args
/usr/local/bin/devmail "$@" &
DEVMAIL_PID=$!

# Wait for SMTP port to be ready (up to 10 seconds)
echo "[test] Waiting for devmail to start..."
for _ in $(seq 1 33); do
    (: < /dev/tcp/127.0.0.1/1025) 2>/dev/null && break
    sleep 0.3
done

# Send test emails
/usr/local/bin/gen-test-emails.sh

# Keep container alive
wait $DEVMAIL_PID
