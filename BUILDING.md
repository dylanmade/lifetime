# Building Lifetime

Lifetime is a Cargo workspace (Rust core/crypto/net/tracker + a Tauri desktop app).
The desktop app builds on macOS and Windows. Platform-specific tracking is
compile-time gated, so the app runs on any supported OS — it just only auto-tracks
app usage on macOS today. On other platforms it still runs, pairs, syncs, and accepts
manual activities, which is all that's needed to be a second device for **sync testing**.

## Common prerequisites (all platforms)

- **Rust** (stable) — https://rustup.rs
- **Node.js 18+** and npm
- A C toolchain (for the bundled SQLCipher/OpenSSL build)

## macOS

```bash
# from the repo root
cd desktop
npm install
npm run tauri dev      # run the app
npm run tauri build    # produce a .app / .dmg
```

## Windows

The desktop app builds on Windows with the MSVC toolchain. The only extra step versus
a typical Tauri app is the SQLCipher dependency, which compiles **OpenSSL from source**
and therefore needs Perl and NASM available on `PATH`.

Install once:

1. **Rust (MSVC)** — `rustup` default toolchain `stable-x86_64-pc-windows-msvc`.
2. **Visual Studio Build Tools** with the "Desktop development with C++" workload
   (provides the MSVC compiler + Windows SDK).
3. **WebView2 Runtime** — preinstalled on Windows 10/11; otherwise install the
   Evergreen runtime from Microsoft.
4. **Node.js 18+**.
5. **Strawberry Perl** — https://strawberryperl.com (needed to build vendored OpenSSL).
6. **NASM** — https://www.nasm.us (assembler for OpenSSL); add its folder to `PATH`.
7. **Tauri CLI**: `npm install` in `desktop/` brings it in via devDependencies.

Then:

```powershell
cd desktop
npm install
npm run tauri dev      # run the app
npm run tauri build    # produce an .msi / .exe installer under src-tauri\target\release\bundle
```

If the build fails inside `openssl-sys`/`openssl-src`, it's almost always a missing
Perl or NASM — confirm `perl --version` and `nasm --version` both work in the same shell.

## Running two instances for sync testing (one machine)

Set `LIFETIME_DATA_DIR` to give each instance an isolated dataset:

```bash
LIFETIME_DATA_DIR=~/lt-A cargo run --manifest-path desktop/src-tauri/Cargo.toml
LIFETIME_DATA_DIR=~/lt-B cargo run --manifest-path desktop/src-tauri/Cargo.toml
```

(`cargo run` uses the pre-built frontend in `desktop/dist`, so run `npm run build` in
`desktop/` first.) For a real two-device test, just run one normal instance on each
machine on the same LAN.

## Sync testing flow

Sync requires encryption (the shared master key is the trust + channel root).

1. On **device A**: Settings → enable encryption; copy the recovery file it shows.
2. On **device B**: Settings → "Sync & devices" → paste A's recovery file + choose a
   local passphrase to **pair** (this installs A's master key on B).
3. On **A**: Settings shows A's sync **port**; note A's LAN IP.
4. On **B**: enter A's `IP` + `port` and **Sync** — B converges to A's data, and further
   edits on either side propagate on the next sync. Each device keeps its own timeline
   distinct (device-scoped reads); amalgamation is opt-in.
