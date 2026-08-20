# Releasing aski

The release loop lives in `~/notes/releasing.md` — the ordered steps, the apt
step, crates.io, the spent-tag rule, and the standing facts about tokens and
secrets. Failure recipes are in `~/notes/build_release_gotchas.md`. This file
carries what is true of aski and not of its siblings.

| | |
|---|---|
| Loop | cargo-dist |
| Version lives in | `version` in `Cargo.toml` |
| `apt-ship` argument | `aski` |
| crate | `aski` |
| winget package | none |
| Windows asset | none |

**The crate, the command, the Homebrew formula, and the apt package are all
`aski`** — one name everywhere. cargo-dist's tarballs and installer are named
after it: `aski-installer.sh`, `aski-<target>.tar.xz`.

**The release builds** four platform tarballs — Linux and macOS, each on x86_64
and aarch64 — plus the shell installer, the Homebrew formula, and the checksums,
then creates the GitHub Release. The `.deb` packages come from the separately
dispatched `deb.yml`.

**Step 6 of the fleet loop does not apply.** aski ships no Windows binary and no
winget manifest, so there is no manifest to submit, no `Excelano.aski` package,
and no `defender-scan.yml` in this repo — that workflow exists only to
pre-flight winget's installation-validation sweep. The release is finished at
apt and crates.io.

That is a decision rather than an omission, and reversing it means two changes
together, not one: adding `x86_64-pc-windows-msvc` and the `powershell` installer
to `dist-workspace.toml`, and teaching `config::path` a Windows location.
Today it resolves `$ASKI_CONFIG`, then `$XDG_CONFIG_HOME`, then `$HOME/.config`,
and on Windows the last two are usually unset, so a Windows build would ship
unable to find a config file. Add the target without the path and the binary
builds, installs, and fails on first run.
