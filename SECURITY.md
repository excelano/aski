# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub Security Advisories at https://github.com/excelano/aski/security/advisories/new. If you would rather not use GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest release receives security fixes. Older versions are not supported.

## What aski can access

aski is a CLI that runs locally on your machine. It reads one configuration file — `$ASKI_CONFIG`, or `$XDG_CONFIG_HOME/aski/config.toml`, or `~/.config/aski/config.toml` — and reads standard input when standard input is a pipe, unless `--no-stdin` refuses it. It then runs the program that configuration file names, passing your question as a command-line argument.

That is the security-relevant fact about aski: **the configuration file names a program to execute.** Anyone who can write to it can make aski run anything your operating-system user can run. Treat it the way you treat `~/.bashrc` or a shell alias file, and do not source one you did not write. aski does reduce the blast radius of a *question* — the argument vector is built directly and handed to `execve`, with no shell in between, so nothing in the text you type is interpreted as a shell metacharacter, a redirection, or a second command.

aski itself makes no network calls, has no auth layer, holds no credentials, and implements no administrative operations. The LLM it launches is a different matter: that program sends your question, your standing `context`, and anything you piped in to whatever service it talks to, under its own credentials and its own privacy terms. aski neither adds to nor limits that. Assume anything you put on the command line reaches a third party, and read the LLM's own policy for what happens next.

## What aski stores

aski stores nothing beyond its configuration file, and it writes that file only when you run `aski --init`, which refuses to overwrite an existing one. There is no history file, no cache, no telemetry, no analytics, and no remote logging. Your questions and the answers are not recorded anywhere by aski; the answer goes to standard output and is gone.

By default each run creates an empty temporary directory, runs the LLM inside it, and removes it on exit. That exists so a one-shot question does not pick up the project you happen to be standing in — several LLM CLIs walk up from the working directory looking for instruction files — and it holds nothing: aski writes no file into it, and the LLM is free to.

## Verifying releases

Every GitHub release includes a `.sha256` file next to each archive listing its SHA-256 hash. Verify any download before running it:

    sha256sum aski-x86_64-unknown-linux-gnu.tar.xz
    # compare against the value in aski-x86_64-unknown-linux-gnu.tar.xz.sha256

Release artifacts are built by GitHub Actions from a tagged commit using the cargo-dist configuration in this repo (`dist-workspace.toml` and the generated `.github/workflows/release.yml`). The workflow and build configuration are public and auditable.
