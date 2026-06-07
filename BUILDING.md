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

Install once, **in this order** (the C++ toolchain must exist before Rust, and the
OpenSSL build tools before the first `cargo build`):

1. **Visual Studio Build Tools** with the "Desktop development with C++" workload
   (provides the MSVC compiler, linker, and Windows SDK).
2. **Rust (MSVC)** — `rustup` default toolchain `stable-x86_64-pc-windows-msvc`.
3. **Strawberry Perl** — https://strawberryperl.com. Needed to build vendored OpenSSL,
   and it must be the Perl that wins on `PATH` (see the warning below).
4. **NASM** — https://www.nasm.us (assembler for OpenSSL); add its folder to `PATH`.
5. **Node.js 18+**.
6. **WebView2 Runtime** — preinstalled on Windows 10/11; otherwise install the
   Evergreen runtime from Microsoft.
7. **Tauri CLI**: `npm install` in `desktop/` brings it in via devDependencies.

> **Run the build from PowerShell or Command Prompt — not Git Bash / MSYS2.** Those
> shells put their own Unix Perl ahead of Strawberry Perl on `PATH`, and that Perl is
> missing modules OpenSSL needs (`Locale::Maketext::Simple`, etc.), which breaks the
> OpenSSL build. Verify with `Get-Command perl` → it should be
> `C:\Strawberry\perl\bin\perl.exe`.

Then (in PowerShell):

```powershell
cd desktop
npm install
npm run tauri dev      # run the app
npm run tauri build    # produce an .msi / .exe installer under src-tauri\target\release\bundle
```

### Troubleshooting

- **`cargo metadata ... program not found`** (or `cargo`/`rustc` "not recognized"):
  the terminal can't find Cargo on its PATH. Install Rust via `rustup`, then **close and
  reopen the terminal** (and your editor if you launch the terminal from it) so
  `%USERPROFILE%\.cargo\bin` is picked up. Verify with `cargo --version` in the same shell
  you run `npm run tauri dev` from.

- **OpenSSL build fails** — `'perl' reported failure` /
  `Can't locate Locale/Maketext/Simple.pm in @INC` (with `/usr/share/perl5/...` paths):
  a Unix Perl from Git Bash / MSYS2 is being used instead of Strawberry Perl. Fix it:
  1. Build from **PowerShell / Command Prompt**, not Git Bash / MSYS2.
  2. Ensure Strawberry Perl wins: `Get-Command perl` → `C:\Strawberry\perl\bin\perl.exe`.
     If not, force it: `$env:OPENSSL_SRC_PERL = "C:\Strawberry\perl\bin\perl.exe"`.
  3. `cargo clean` (or delete `target\debug\build\openssl-sys-*`) so OpenSSL re-configures,
     then rebuild.

  Sanity check: `perl -MLocale::Maketext::Simple -e 1` succeeds on Strawberry Perl.

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
