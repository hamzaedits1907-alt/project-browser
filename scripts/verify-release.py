#!/usr/bin/env python3
"""Independent check of a published Project Browser release (used by the release workflow).

It does not trust the build: it downloads `latest.json` and the installer exactly as the in-app updater
would, then checks that

  1. latest.json parses and announces the version of the tag that was just released,
  2. the Windows entry points at a downloadable installer,
  3. the signature in latest.json was made by the key whose public half is in tauri.conf.json
     (Minisign / Ed25519, verified here with the `cryptography` package, not by Tauri), and covers
     exactly the downloaded bytes.

usage:
  verify-release.py --tag v0.1.1 [--repo owner/name] [--manifest-url URL] [--out report.json]
  verify-release.py --selftest        # checks the verifier itself against built-in fixtures

Exit code 0 = everything verified, 1 = a check failed.
"""
import argparse
import base64
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

REPO_ROOT = Path(__file__).resolve().parent.parent


class VerifyError(Exception):
    pass


def _lines(b64_text: str) -> list:
    return base64.b64decode(b64_text.strip()).decode("utf-8").splitlines()


def parse_pubkey(pubkey_b64: str):
    """Returns (key_id: bytes, ed25519 public key bytes)."""
    lines = _lines(pubkey_b64)
    raw = base64.b64decode(lines[1])
    if raw[:2] != b"Ed" or len(raw) != 42:
        raise VerifyError("public key is not a minisign Ed25519 key")
    return raw[2:10], raw[10:]


def verify_minisign(data: bytes, signature_b64: str, pubkey_b64: str) -> str:
    """Verifies a Tauri/minisign signature (the contents of a `.sig` file, base64) over `data`.

    Returns the trusted comment. Raises VerifyError when anything does not match.
    """
    key_id, pk = parse_pubkey(pubkey_b64)
    lines = _lines(signature_b64)
    if len(lines) < 4:
        raise VerifyError("signature file has the wrong shape")
    sig_raw = base64.b64decode(lines[1])
    algorithm, sig_key_id, sig = sig_raw[:2], sig_raw[2:10], sig_raw[10:]
    if len(sig) != 64:
        raise VerifyError("signature has the wrong length")
    if sig_key_id != key_id:
        raise VerifyError(f"signed by key {sig_key_id[::-1].hex().upper()}, but the app trusts {key_id[::-1].hex().upper()}")
    message = hashlib.blake2b(data, digest_size=64).digest() if algorithm == b"ED" else data
    if algorithm not in (b"ED", b"Ed"):
        raise VerifyError("unknown signature algorithm")
    public = Ed25519PublicKey.from_public_bytes(pk)
    try:
        public.verify(sig, message)
    except InvalidSignature:
        raise VerifyError("signature does not match the downloaded file") from None
    trusted = lines[2]
    if not trusted.startswith("trusted comment: "):
        raise VerifyError("missing trusted comment")
    global_sig = base64.b64decode(lines[3])
    try:
        public.verify(global_sig, sig + trusted[len("trusted comment: "):].encode("utf-8"))
    except InvalidSignature:
        raise VerifyError("trusted comment does not match its signature") from None
    return trusted[len("trusted comment: "):]


def fetch(url: str, limit: int = 600 * 1024 * 1024) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "project-browser-release-check"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        data = resp.read(limit + 1)
    if len(data) > limit:
        raise VerifyError(f"{url} is unreasonably large")
    return data


def configured_pubkey() -> str:
    conf = json.loads((REPO_ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    return conf["plugins"]["updater"]["pubkey"]


def check_release(tag: str, manifest_url: str, report: dict) -> None:
    version = tag.lstrip("v")
    raw = fetch(manifest_url)
    manifest = json.loads(raw)
    report["manifest"] = {k: manifest[k] for k in ("version", "notes", "pub_date") if k in manifest}
    if manifest.get("version") != version:
        raise VerifyError(f"latest.json announces {manifest.get('version')!r}, expected {version!r}")
    platforms = manifest.get("platforms", {})
    entry = platforms.get("windows-x86_64-nsis") or platforms.get("windows-x86_64")
    if not entry:
        raise VerifyError(f"no Windows entry in latest.json (platforms: {sorted(platforms)})")
    url, signature = entry["url"], entry["signature"]
    report["installer_url"] = url
    if not url.startswith("https://"):
        raise VerifyError("installer URL is not https")
    package = fetch(url)
    report["installer_bytes"] = len(package)
    report["installer_sha256"] = hashlib.sha256(package).hexdigest()
    if len(package) < 1_000_000:
        raise VerifyError("installer is suspiciously small")
    trusted = verify_minisign(package, signature, configured_pubkey())
    report["trusted_comment"] = trusted
    report["signature_verified_against_configured_key"] = True


def selftest() -> None:
    # Fixtures: a throwaway key pair (not the release key) signing a tiny payload. Same ones as src-tauri/src/updates.rs.
    pub_a = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEVGNzgxQkNGMEQ1MUIwODcKUldTSHNGRU56eHQ0NyszRFZSdHpkbWtLR01GQ0lmY2NFdTVDVkxsTGprMUxIRGhBOFpCNG5qK3YK"
    pub_b = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk0ODQzMDQ4NTVFMjRCNTAKUldSUVMrSlZTRENFbElRNXF5VUZkc3FxdERqN01yZHIwWW8ycnRLTzZXbmVSVWFlQXBTYXEyTSsK"
    payload = b"project-browser-test-update-payload-v1"
    sig = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTSHNGRU56eHQ0NzlRVzNvV1p2b0JrdVZaMEEwUk9BWWFWWHFnUXYrdUhvQjJ6ekhPUVUzdHVqRG00UG03ZURTV3l6dmczYXBVUEF6MXY3a3l1TmxRRHE1N09EV1lhUHdjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNTgxNDg0CWZpbGU6cGF5bG9hZC5iaW4KQ2lUTTJhbUJld2plcHl6LzZveEtCQWozQlM0WGlWNHU0Z2U4RlBFL2pKcDB0OTlOaWt3TGFaUUtDQWFZcVY1ZndoMExvYjhzSkV5UW1PWFU1MGNCRFE9PQo="
    assert "payload.bin" in verify_minisign(payload, sig, pub_a), "valid signature must verify"
    for name, args in {
        "tampered file": (payload + b"x", sig, pub_a),
        "wrong key": (payload, sig, pub_b),
    }.items():
        try:
            verify_minisign(*args)
        except VerifyError:
            continue
        raise SystemExit(f"selftest FAILED: {name} was accepted")
    print("selftest passed: valid signature accepted; tampered file and wrong key rejected")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag")
    ap.add_argument("--repo", default="hamzaedits1907-alt/project-browser")
    ap.add_argument("--manifest-url")
    ap.add_argument("--out")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        selftest()
        return 0
    if not args.tag:
        ap.error("--tag is required")
    url = args.manifest_url or f"https://github.com/{args.repo}/releases/latest/download/latest.json"
    report = {"tag": args.tag, "manifest_url": url}
    try:
        check_release(args.tag, url, report)
        report["result"] = "verified"
        code = 0
    except (VerifyError, KeyError, ValueError, OSError) as e:
        report["result"] = f"FAILED: {e}"
        code = 1
    print(json.dumps(report, indent=2))
    if args.out:
        Path(args.out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    return code


if __name__ == "__main__":
    sys.exit(main())
