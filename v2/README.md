# Bitly URL Shortener — external Rust V2

Implementation for independent review on `feat/native-v2-rust`. This is an external schema-3, protocol-1 executable, not a compiled-in Tezbar adapter. It has not been installed, published or released. The source manifest intentionally has `artifacts: {}`; the ignored local ARM64 ZIP is a development package, not a downloadable catalog release.

The only original-file change is the short root README link. All implementation, evidence, resources, notices and inactive CI material live in `v2/`. Root V1 files, lockfile and source HEAD remain unchanged.

## Commands and preferences

- `index` / **Shorten URL** automatically reads selected text (empty/rejected selection falls back to clipboard), trims using ECMAScript whitespace rules, validates an absolute URL, sends the original literal to Bitly, copies the returned shortened URL, reports success, then optionally pastes. Successful launch returns no view. Failures show a deliberate recovery action when appropriate; no confirmation screen was inserted into normal launch.
- `list-links` / **List Links** loads one complete Bitly group page, performs server search, shows title, shortened link and creation date, and offers **Copy Bitlink**, **Open in Bitly** and the original empty-state **Create a Bitly Link**. The locked original hook does not configure pagination. API-provided `pagination.next` is never followed. No delete feature is declared or implemented.
- Extension preferences remain `accessToken` (password) and `pasteAfterShortening` (checkbox, false). `group_guid` remains a required command preference for List Links. There is no invented domain/group preference for shortening.

Original `new URL()` accepts custom schemes, so input validation retains that behavior; browser actions accept only safe HTTP(S) data and open a constructed authentic `https://app.bitly.com/` destination. JSON bodies and search queries are encoded correctly. Search punctuation is treated as text rather than the original raw query interpolation. Remote titles remain plain React text, and literal clipboard content is never converted to HTML. Remote favicons are intentionally omitted to avoid sending original URLs to another service; all three original PNG command assets are included unchanged.

## Failure and cancellation behavior

The HTTP client uses verified Rustls TLS, no proxies, no redirect following, a fixed authentic Bitly API origin, a five-second connection deadline and fifteen-second total request deadline. No POST is automatically retried. Limits: 8 KiB input/row strings, 1 KiB search text, 2 MiB response, 32 JSON levels, 30,000 JSON nodes, 1,000 rows on the original single page. Invalid/partial/contradictory data rejects the whole page. API and host error details are replaced with safe category messages; 401 retains **Invalid Access Token** and **Open Command Preferences**, while 403 remains a generic permission/plan error.

Before POST, an atomic uncertainty marker is written to SDK-private `dataDir/bitly-v2/recovery.json`. It contains a length-delimited SHA256 scope key (private data directory, credential and original input) and either uncertainty or the returned shortened link plus copy acknowledgement. It never stores the credential, original input or group data. Malformed, oversized and symlinked store paths fail closed. A shared transaction mutex and atomic tempfile replacement prevent lost updates within the host's one-process-per-profile ownership contract. The store is at most 128 KiB / 64 records. Unknown creation and unacknowledged copy records are never evicted to make room; acknowledged copies may be evicted at capacity. If every slot is unresolved, new shortening fails safely.

If a shortening request times out, is canceled, or loses its response, reopening the same scoped original input shows **Retry Shortening (may create duplicate)** and the explicit duplication warning. Refresh/reopen cannot silently resend it. A saved response permits exact-byte copy/paste retry without another POST. Clipboard and paste retries are deliberate: a missing acknowledgement does not prove the external side effect did not happen. `pasteText` already handles write/hide/focus/paste; the adapter does not duplicate those OS actions. A retained acknowledged link is reused on a matching reopen while it remains in the bounded store.

This is not a distributed transaction. A remote link can be created despite cancellation. Disk failure, externally removed recovery data, a changed credential/original input, eviction of an acknowledged record, or a different profile prevents cross-boundary recovery. Once that information is gone, the extension cannot discover an unknown created link automatically. Check List Links before deliberately retrying. The host must not replay failed actions. SDK staged state does not roll back HTTP, disk or clipboard effects.

Per-session action IDs are one-use and consumed before awaits. Beginning a search clears old rows/actions; SDK latest-search fencing plus an authoritative bounded session map prevents discarded responses from authorizing old actions. Synchronous drop guards remove canceled-open state. No detached extension tasks, helper processes, polling timers or background leases are used. Views use `refreshMs: null`. The eight-entry list cache expires after thirty seconds and is scoped by private directory, token, group and query; it is process-local, never a persistent original-link database. No V1 storage migration is claimed.

## Immutable evidence and validation

- Own source: `d946df7d3643a05f2bb3fadd12edf0b8a3858598`.
- SDK 0.1.1: HTTPS Git `e955911f0bf5b558f9d3eb5183df0adc95c22af2`, Cargo.lock included; MSRV 1.85.
- Host contract: `ea100cee3f69711de12485ba061950c5fc65258f`, read only with `git show`.
- Raycast source/assets: `6734d85992290066fd31a7f5bee3cca3d3545a1a`.
- Original icon SHA256: `5d2e2c18827fc67fa583feddde2b057bb9b3c4e6c52ad916a3fc5fdfa3c5967f`.

See [QA.md](QA.md) for actual checks, [INTEGRATION.md](INTEGRATION.md) for host acceptance gaps and the separate unapplied patch, [evidence/provenance.json](evidence/provenance.json) and [evidence/assets.json](evidence/assets.json) for object/asset hashes, and [INDEX.md](INDEX.md) for every Git-visible file. `evidence/source-oracle.json`, `evidence/url-oracle.json` and `evidence/protocol-view.json` are executed synthetic evidence, not live service captures. [NOTICE](NOTICE) and [licenses](licenses) preserve authentic texts and explicitly record QA-only license-text gaps.

## Reproduce focused checks

Run from the clone root. All generated data belongs to ignored `v2/qa`, `v2/target` or `v2/dist`; no installer or release step exists.

```sh
python3 v2/scripts/prepare-v1.py
HOME="$PWD/v2/qa/home" TMPDIR="$PWD/v2/qa/tmp" npm ci --prefix v2/qa/v1 --ignore-scripts --cache "$PWD/v2/qa/npm-cache" --no-audit --no-fund
HOME="$PWD/v2/qa/home" TMPDIR="$PWD/v2/qa/tmp" npm ci --prefix v2/qa/tooling --ignore-scripts --cache "$PWD/v2/qa/npm-cache" --no-audit --no-fund
node v2/scripts/source-oracle.cjs
cargo fmt --manifest-path v2/Cargo.toml --check
cargo test --locked --manifest-path v2/Cargo.toml --all-targets --features qa
cargo clippy --locked --manifest-path v2/Cargo.toml --all-targets --features qa -- -D warnings
cargo build --locked --manifest-path v2/Cargo.toml --bins --features qa
python3 v2/scripts/protocol-qa.py
node v2/scripts/host-fixture.cjs
python3 v2/scripts/host-registry-fixture.py
```

Repeat Rust tests/build and process QA with `cargo +1.85.0`. `qa` enables a separate loopback-only test executable; production has no endpoint/trust environment override. The private synthetic TLS test key in `tests/tls` is only a test fixture, not a credential, and is not packaged. QA executes original TSX and locked utils with a closed VM producer map; native fs/os/process/network/browser/clipboard are denied or injected. It never runs original JavaScript against live services.

Production ARM64 and local extraction checks:

```sh
cargo build --locked --manifest-path v2/Cargo.toml --release --target aarch64-apple-darwin --bin bitly-url-shortener
python3 v2/scripts/licenses.py
python3 v2/scripts/package.py
python3 v2/scripts/protocol-qa.py --production --binary v2/dist/extracted/bin/bitly-url-shortener
python3 v2/scripts/isolated-build.py
```

The production smoke uses synthetic private prepared-response data for a successful copy, and invalid-token/URL failure flows; it makes no real Bitly request and performs no real OS clipboard/browser/paste action. Actual remote creation behavior is tested only against local HTTP using the QA fixture. Windows GNU check is blocked by the absent MinGW `ring` C compiler; MSVC and live desktop acceptance have not run. `scripts/ci.yml.example` is inactive and publishes nothing.
