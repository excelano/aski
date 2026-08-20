# aski — one-shot questions for command-line LLMs

You are in the middle of something at the shell and a question comes up. `aski` puts it to whichever LLM you have installed, prints the answer, and gets out of the way. There is no session, no follow-up, and no quoting: the question is just the rest of the command line.

```sh
$ aski how do I extract a tar.zst into ./out
tar -xf archive.tar.zst -C ./out
```

It is a generalization of a small `claude -p` wrapper. Every LLM is a table in a TOML config file, so adding one is a matter of writing down the argv that puts it in one-shot mode.

## Install

### Debian and Ubuntu

Add the [Excelano apt repository](https://excelano.com/apt/) once (one-time setup):

```sh
curl -fsSL https://excelano.com/apt/setup.sh | sudo sh
```

Then install it, so `apt upgrade` keeps it current:

```sh
sudo apt install aski
```

Both amd64 and arm64 packages ship with every release.

### Homebrew

On macOS or Linux, tap and trust the repository once — Homebrew gates third-party taps behind explicit trust (one-time setup):

```sh
brew tap excelano/tap
brew trust excelano/tap
```

Then install it, so `brew upgrade` keeps it current:

```sh
brew install aski
```

### Prebuilt binary (Linux and macOS)

```sh
curl -fsSL https://raw.githubusercontent.com/excelano/aski/main/install.sh | sh
```

The installer downloads the right tarball for your platform from the GitHub release, verifies its checksum, and drops the binary into `~/.cargo/bin`. If `aski` isn't found on your `PATH` afterward, ensure `~/.cargo/bin` is on it. Releases also ship raw tarballs (`aski-*.tar.xz`) for manual installation. To uninstall:

```sh
curl -fsSL https://raw.githubusercontent.com/excelano/aski/main/uninstall.sh | sh
```

That removes the binary and leaves your config file alone.

### From crates.io

```sh
cargo install aski
```

### Windows

There is no Windows build. aski looks for its config through `XDG_CONFIG_HOME` and `HOME`, which are usually unset there, so a Windows binary would install and then fail to find a config file. Adding the target means adding a Windows config path in the same change.

## First run

Write the starter config, which comes with a Claude entry already filled in:

```sh
aski --init
```

That creates `~/.config/aski/config.toml` (or `$XDG_CONFIG_HOME/aski/config.toml`, or whatever `$ASKI_CONFIG` points at). It refuses to overwrite a config that already exists.

## Using it

The first argument selects an LLM. If it matches a table name or an alias, it is consumed as the selector and everything after it is the question. If it matches nothing, it is the first word of the question and the `default` LLM answers.

```sh
aski how do I find files modified in the last day    # the default LLM
aski gemini what does the --depth flag do            # a named LLM
aski c explain this awk one-liner: NR % 2            # an alias
```

Occasionally a question genuinely starts with an LLM's name. `--` forces the default and treats the rest as text:

```sh
aski -- claude code keeps asking for permissions
```

Context can also arrive on stdin, which is where a question about output you are staring at usually comes from:

```sh
somecommand --help | aski what does the --depth flag do
git log --oneline -20 | aski summarize what changed here
```

The piped text is appended below the typed question. When stdin is a pipe, `aski` drains it before starting the LLM, so avoid running it non-interactively with an inherited stdin that never closes.

The remaining options are `--list`, which prints the configured LLMs and the command each one runs, plus the usual `-h`/`--help` and `-V`/`--version`. All four are recognized only as the first argument, so `aski what does --help do` asks the question rather than printing usage.

Quoting is needed only for the shell's sake — a question containing `?`, `*`, `|`, `>`, `$`, or an apostrophe still has to be quoted, because `aski` never sees those characters otherwise.

## Configuring an LLM

Each `[llm.<name>]` table describes one command-line LLM. The table name is the selector you type.

```toml
default = "claude"

[llm.claude]
aliases = ["c"]
command = [
    "claude",
    "-p",
    "--model", "haiku",
    "--append-system-prompt", "{context}",
    "{prompt}",
]
context = "Answer in a few lines. Lead with the exact command. No preamble."

[llm.ollama]
aliases = ["o"]
command = ["ollama", "run", "llama3.2", "{prompt}"]
context = "Answer in a few lines. Lead with the exact command."
neutral_dir = false
```

`command` is argv, spawned directly with no shell in between, so each element is one argument and needs no quoting beyond TOML's own. Two placeholders are substituted anywhere they appear, including inside a longer string such as `"Question: {prompt}"`.

| Key           | Required | Meaning |
|---------------|----------|---------|
| `command`     | yes      | argv for the LLM. At least one element must contain `{prompt}`. |
| `context`     | no       | Standing text sent with every question. |
| `aliases`     | no       | Extra selectors, for shorter typing. |
| `neutral_dir` | no       | Run in an empty temporary directory. Defaults to true. |

Where the context goes depends on the command. If any element contains `{context}`, the context is substituted there, which is how you reach a flag like `--append-system-prompt`. If no element does, the context is prepended to the question with a blank line between, which works for any LLM that takes only a bare prompt.

`neutral_dir` exists because some LLM CLIs walk up from the working directory looking for project instructions. A one-shot terminal question should not drag in whatever repository you happen to be standing in, so by default the LLM runs in an empty temporary directory that is removed when it exits. Turn it off for an LLM whose configuration lives in the directory you are in.

Configuration errors are caught before anything is spawned: a `default` that names no table, a `command` with no `{prompt}`, a `{context}` with no context set, an alias that collides with another selector, or a misspelled key all fail with a message naming the file and the LLM.

## License

MIT. See [LICENSE](LICENSE).
