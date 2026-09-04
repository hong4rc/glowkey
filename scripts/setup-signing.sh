#!/usr/bin/env bash
# Creates the stable code-signing identity GlowKey builds with, once.
#
#   bash scripts/setup-signing.sh
#
# Idempotent: if the identity already exists this prints and exits.
#
# ## Why this exists
#
# Without it, builds are **ad-hoc signed**, and macOS keys the Accessibility
# grant to the signature's cdhash — which changes with every code change. So
# every install is a brand-new app as far as the system is concerned, the grant
# is dropped, and you go back to System Settings. Again.
#
# A stable self-signed certificate gives the bundle a designated requirement that
# names the identifier and the certificate rather than a hash of the code, and
# that does not move when the code does.
#
# ## What it does not buy
#
# Gatekeeper. A self-signed certificate is not a Developer ID, so a *downloaded*
# copy still needs `xattr -dr com.apple.quarantine`. This is only about the
# machine that builds it. See docs/decisions/0006-stable-signing-identity.md.
set -euo pipefail

IDENTITY="${GLOWKEY_SIGN_IDENTITY:-GlowKey Developer}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

# Exit 0 means "already there", exit 10 means "just created it". The caller uses
# that to decide whether the stale ad-hoc grant needs clearing — which must
# happen exactly once, on the run that changes how the app is signed.
# Captured, not piped into `grep -q`: under `pipefail` grep exits at the first
# match without draining, `security` takes SIGPIPE, and the pipeline reports 141
# — a failure — even though the identity was found.
#
# `-p codesigning` without `-v`: a self-signed certificate is not *trusted*, so
# `-v` ("valid identities only") hides it — but trust governs signature
# verification, not signing. codesign uses it happily, and the designated
# requirement it produces names the identifier and the certificate rather than a
# hash of the code, which is the entire point.
have_identity() {
    local ids
    ids="$(security find-identity -p codesigning 2>/dev/null || true)"
    [ "${ids#*"$IDENTITY"}" != "$ids" ]
}

if have_identity; then
    echo "==> Signing identity \"$IDENTITY\" already exists — nothing to do."
    exit 0
fi

echo "==> Creating the code-signing identity \"$IDENTITY\""

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
# A throwaway passphrase for the PKCS#12 container, which exists only long enough
# to be imported. Never printed, never written outside this temp directory.
# Not `tr </dev/urandom | head -c 32`: `head` exits at 32 bytes, `tr` takes
# SIGPIPE, and `pipefail` turns that into a failed script. Same trap as the
# identity lookup below, and as `build-app.sh` before it.
P12_PASS="$(openssl rand -hex 24)"

# `extendedKeyUsage = codeSigning` is the part that makes `security
# find-identity -p codesigning` list it; a plain self-signed certificate without
# it is invisible to codesign.
cat > "$WORK/openssl.cnf" <<'CNF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = GlowKey Developer
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
# Ten years: the identity expiring silently drops the build back to ad-hoc
# signing, and the only symptom is the "no signing identity" line scrolling past.
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
    -config "$WORK/openssl.cnf" 2>/dev/null

openssl pkcs12 -export -legacy \
    -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
    -name "$IDENTITY" -out "$WORK/identity.p12" \
    -passout "pass:$P12_PASS" 2>/dev/null \
  || openssl pkcs12 -export \
    -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
    -name "$IDENTITY" -out "$WORK/identity.p12" \
    -passout "pass:$P12_PASS" 2>/dev/null

# `-T /usr/bin/codesign` pre-authorises codesign to use the key. Deliberately
# not `-A`, which would authorise every program on the machine.
security import "$WORK/identity.p12" -k "$KEYCHAIN" \
    -T /usr/bin/codesign -P "$P12_PASS" >/dev/null

# Captured, not piped into `grep -q`: under `pipefail` grep exits at the first
# match without draining, `security` takes SIGPIPE, and the pipeline reports 141
# — a failure — even though the identity was found.
#
# `-p codesigning` without `-v`: a self-signed certificate is not *trusted*, so
# `-v` ("valid identities only") hides it — but trust governs signature
# verification, not signing. codesign uses it happily, and the designated
# requirement it produces names the identifier and the certificate rather than a
# hash of the code, which is the entire point.
have_identity() {
    local ids
    ids="$(security find-identity -p codesigning 2>/dev/null || true)"
    [ "${ids#*"$IDENTITY"}" != "$ids" ]
}

if have_identity; then
    echo "==> Done. Builds will now sign with \"$IDENTITY\"."
    echo "    The first build may ask permission to use the key — choose"
    echo "    \"Always Allow\" and it will not ask again."
    exit 10
else
    echo "the identity was imported but codesign cannot see it" >&2
    exit 1
fi
