#!/usr/bin/env bash
# Sends 10 Lorem Ipsum-style test emails to devmail via swaks.
# Targets ~50% of the default 32 MB inbox limit (~16 MB total).
#
# Size breakdown:
#   7 plain-text / HTML emails:  ~10 KB each  →  ~70 KB
#   email 4 attachment (txt):    ~1 MB raw    →  ~1.4 MB stored (base64)
#   email 7 attachment (csv):    ~4 MB raw    →  ~5.4 MB stored (base64)
#   email 10 attachment (txt):   ~7 MB raw    →  ~9.4 MB stored (base64)
#   Total stored:                             ~16.3 MB  ✓
set -euo pipefail

SMTP="127.0.0.1:1025"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ── Lorem Ipsum paragraphs ────────────────────────────────────────────────────
P1="Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat."

P2="Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum."

P3="Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo."

P4="Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est qui dolorem ipsum quia dolor sit amet."

LOREM_BODY="$P1

$P2

$P3

$P4"

# ── Attachment files ───────────────────────────────────────────────────────────
echo "[test] Generating attachment files..."

# quarterly_report.txt — financial report (~1 MB)
{
  printf "ACME Corp — Q1 2026 Quarterly Financial Report\n"
  printf "Prepared by: Finance Department | Date: March 20, 2026\n"
  printf "Confidential — For Internal Use Only\n\n"
  printf "════════════════════════════════════════════════════════════\n\n"
  yes "$LOREM_BODY

── Revenue Summary ──────────────────────────────────────────
Q1 Total Revenue:   \$4,820,000  (+12% YoY)
Q1 Operating Cost:  \$3,105,000  (+7% YoY)
Q1 Net Income:      \$1,715,000  (+22% YoY)

── Department Highlights ────────────────────────────────────
$P3

── Outlook ──────────────────────────────────────────────────
$P4

" | head -c 1000000 || true
} > "$TMP/quarterly_report.txt"

# analytics_march_2026.csv — employee analytics data (~4 MB)
CSV_ROWS='1042,Alice Johnson,alice.johnson@acme.example,Engineering,New York,2019-04-15,105000,Active,"Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor"
1043,Bob Smith,bob.smith@acme.example,Marketing,Chicago,2020-07-22,88000,Active,"Ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi aliquip"
1044,Carol White,carol.white@acme.example,Finance,Boston,2018-11-30,112000,Active,"Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore"
1045,David Brown,david.brown@acme.example,Operations,Austin,2021-03-10,79000,Active,"Excepteur sint occaecat cupidatat non proident sunt in culpa qui officia"
1046,Emma Davis,emma.davis@acme.example,HR,Seattle,2022-08-05,82000,Active,"Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium"
1047,Frank Miller,frank.miller@acme.example,Legal,Denver,2017-06-18,125000,Active,"Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit"
1048,Grace Lee,grace.lee@acme.example,Engineering,New York,2023-01-09,98000,Active,"At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis"
1049,Henry Wilson,henry.wilson@acme.example,Sales,Miami,2020-09-14,91000,Active,"Nam libero tempore cum soluta nobis est eligendi optio cumque nihil impedit"'
{
  echo "id,name,email,department,location,hire_date,salary,status,notes"
  yes "$CSV_ROWS" | head -c 4190000 || true
} > "$TMP/analytics_march_2026.csv"

# Q1_2026_AllHands.txt — presentation outline (~7 MB)
SLIDE_BLOCK="━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
SLIDE CONTENT
$P1

SPEAKER NOTES
$P2

KEY TAKEAWAYS
• $P3
• $P4

"
{
  printf "Q1 2026 ALL-HANDS MEETING — Presentation Notes\n"
  printf "Date: Friday, March 20, 2026 | 3:00 PM EST\n"
  printf "Presenter: Diana, VP Operations\n\n"
  printf "════════════════════════════════════════════════════════════\n\n"
  yes "$SLIDE_BLOCK" | head -c 7330000 || true
} > "$TMP/Q1_2026_AllHands.txt"

# ── Helper: send one email, log result ────────────────────────────────────────
send() {
    local n="$1"; shift
    if swaks --server "$SMTP" --suppress-data "$@" > /dev/null 2>&1; then
        echo "[test] $n/10 sent"
    else
        echo "[test] $n/10 FAILED"
    fi
}

echo "[test] Sending 10 test emails..."

# 1 — Plain text
send 1 \
    --from "alice@acme.example" --to "dev@devmail.test" \
    --header "Subject: Project Update — Q1 Planning" \
    --body "$P1

$P2

Regards,
Alice"

# 2 — HTML
cat > "$TMP/email2.eml" << EOF
From: newsletter@marketing.example
To: dev@devmail.test
Subject: Weekly Newsletter: Lorem Ipsum Digest
MIME-Version: 1.0
Content-Type: text/html; charset=UTF-8

<html><body>
<h2 style="color:#2d3748">Lorem Ipsum Weekly</h2>
<p>$P1</p>
<p>$P2</p>
<hr>
<p style="color:#888;font-size:12px">You are receiving this because you subscribed to the Lorem Ipsum digest.</p>
</body></html>
EOF
send 2 \
    --from "newsletter@marketing.example" --to "dev@devmail.test" \
    --data "$TMP/email2.eml"

# 3 — Plain text
send 3 \
    --from "bob@partner.example" --to "dev@devmail.test" \
    --header "Subject: Re: Lorem ipsum follow-up" \
    --body "Hi,

$P3

Let me know your thoughts.

Best regards,
Bob"

# 4 — Plain text WITH attachment (~1 MB → ~1.4 MB stored)
send 4 \
    --from "charlie@finance.example" --to "dev@devmail.test" \
    --header "Subject: Q1 Quarterly Report" \
    --body "Hi team,

Please find the Q1 quarterly report attached for your review.

$P4

Thanks,
Charlie" \
    --attach "@$TMP/quarterly_report.txt" \
    --attach-type "text/plain" \
    --attach-name "quarterly_report.txt"

# 5 — HTML
cat > "$TMP/email5.eml" << EOF
From: alice@acme.example
To: team@devmail.test
Subject: Action Required: Please Review Before Friday
MIME-Version: 1.0
Content-Type: text/html; charset=UTF-8

<html><body>
<h3 style="color:#c0392b">Action Required</h3>
<p>$P1</p>
<ul>
  <li>Review the attached document</li>
  <li>Submit your feedback by Friday</li>
  <li>Join the call at 2pm EST</li>
</ul>
<p>$P2</p>
<p>Thanks,<br><strong>Alice</strong></p>
</body></html>
EOF
send 5 \
    --from "alice@acme.example" --to "team@devmail.test" \
    --data "$TMP/email5.eml"

# 6 — Plain text
send 6 \
    --from "diana@ops.example" --to "dev@devmail.test" \
    --header "Subject: Infrastructure Maintenance — Saturday 02:00 UTC" \
    --body "Team,

$P2

$P3

Affected systems will be offline from 02:00–04:00 UTC on Saturday.

— Diana, Ops"

# 7 — Plain text WITH attachment (~4 MB → ~5.4 MB stored)
send 7 \
    --from "data@analytics.example" --to "dev@devmail.test" \
    --header "Subject: Analytics Dataset — March 2026" \
    --body "Hi,

This month's full analytics dataset is attached.

$P1

Please load it into the dashboard before the Friday review.

Cheers,
Analytics Team" \
    --attach "@$TMP/analytics_march_2026.csv" \
    --attach-type "text/csv" \
    --attach-name "analytics_march_2026.csv"

# 8 — HTML
cat > "$TMP/email8.eml" << EOF
From: bob@partner.example
To: dev@devmail.test
Subject: Partnership Proposal — Acme & Partner Co.
MIME-Version: 1.0
Content-Type: text/html; charset=UTF-8

<html><body>
<h2>Partnership Proposal</h2>
<p>$P3</p>
<p>$P4</p>
<p>We look forward to hearing from you.</p>
<p>Kind regards,<br><strong>Bob</strong><br>Partner Co.</p>
</body></html>
EOF
send 8 \
    --from "bob@partner.example" --to "dev@devmail.test" \
    --data "$TMP/email8.eml"

# 9 — Plain text
send 9 \
    --from "charlie@finance.example" --to "dev@devmail.test" \
    --header "Subject: Invoice #10042 — Due March 31" \
    --body "Dear Team,

Invoice #10042 for \$4,200.00 is due by March 31, 2026.

$P4

Please process payment at your earliest convenience.

Finance Team"

# 10 — HTML WITH attachment (~7 MB → ~9.4 MB stored)
cat > "$TMP/email10.eml" << OUTER
From: diana@ops.example
To: all@devmail.test
Subject: All-Hands Deck: Q1 2026 Review
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="==boundary=="

--==boundary==
Content-Type: text/html; charset=UTF-8

<html><body>
<h2>Q1 2026 All-Hands</h2>
<p>$P1</p>
<p>$P3</p>
<p>Please review the attached presentation notes before the meeting on Friday at 3pm.</p>
<p>— Diana</p>
</body></html>
--==boundary==
Content-Type: text/plain; charset=UTF-8
Content-Transfer-Encoding: base64
Content-Disposition: attachment; filename="Q1_2026_AllHands.txt"

$(base64 "$TMP/Q1_2026_AllHands.txt")
--==boundary==--
OUTER
send 10 \
    --from "diana@ops.example" --to "all@devmail.test" \
    --data "$TMP/email10.eml"

echo "[test] Done — visit http://localhost:8085"
