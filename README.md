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

The piped text is appended below the typed question. When stdin is a pipe, `aski` drains it before starting the LLM, so an inherited stdin that never closes will hang a non-interactive run; `--no-stdin` is the way out of that.

Options lead. Parsing stops at the first word that is not a flag, so a flag inside a question is still question text and `aski what does --help do` asks rather than printing usage. A question that genuinely starts with a dash needs `--` in front of it, the same escape that stops the first word from selecting an LLM.

```sh
aski -- --depth is a flag of what
```

`--list` prints the configured LLMs and the command each one runs, and `-h`/`--help` and `-V`/`--version` do the obvious thing. Each of those answers the invocation by itself, so none can be combined with a question.

Quoting is needed only for the shell's sake — a question containing `?`, `*`, `|`, `>`, `$`, or an apostrophe still has to be quoted, because `aski` never sees those characters otherwise.

## In a script

Everything that makes aski pleasant to type makes it ambiguous to call from a script. The first word may or may not select an LLM, a mistyped LLM name silently becomes part of the question instead of failing, and a question read out of a variable is text nobody inspected. These flags take the guessing out.

```sh
answer=$(aski --llm claude --timeout 60s -- "$question")
```

`-l`/`--llm` names the LLM outright. It fails when the name is not configured rather than folding it into the question the way a first word does, and it stops the first word from being read as a selector at all. `ASKI_LLM` does the same job from the environment, with one difference: it replaces the config's `default` rather than suppressing positional selection, so a script that wants no guessing anywhere should use the flag or `--`.

`--timeout` bounds the wait. A bare number is seconds, and `90s`, `2m` and `1h` spell it out. The LLM is killed when the limit passes and aski exits 124. Only the LLM's own process is signalled, so one that spawns children of its own can leave them behind.

`--no-stdin` refuses standard input, both as context for the question and as a handle for the LLM. aski otherwise reads stdin whenever it is not a terminal, which is right at a prompt and a hang waiting to happen in a script or a service whose inherited stdin never closes.

`--context` replaces the LLM's standing context for one run and `--no-context` drops it, so a single configured LLM can answer in whatever voice a given script needs. An LLM whose command carries the context in a `{context}` placeholder cannot run without one, and says so rather than passing an empty argument along.

`--here` runs the LLM in the current directory and `--neutral-dir` forces the empty temporary one, either way overriding that LLM's `neutral_dir` setting.

`-n`/`--dry-run` prints the command aski would run, shell-quoted on one line, and stops. It reads stdin and builds the prompt first, so the line it prints is the one that would have been spawned.

### Exit codes

aski is a wrapper, so it borrows the convention `timeout(1)` and `env(1)` use: the LLM's own exit code passes through untouched, and aski's own troubles land in the high band above it.

| Code    | Meaning |
|---------|---------|
| 0-123   | the LLM's exit code, passed through |
| 124     | the LLM was killed by `--timeout` |
| 125     | aski itself failed: no config, an unknown option, an LLM name that is not configured |
| 126     | the LLM's command was found but could not be run |
| 127     | the LLM's command was not found |
| 128+n   | the LLM was killed by signal n |

An LLM that exits 124 through 127 of its own accord is indistinguishable from aski reporting one of those, which is the same ambiguity `timeout(1)` documents and lives with.

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

| Key           | Required | Meaning | Overridden per run by |
|---------------|----------|---------|-----------------------|
| `command`     | yes      | argv for the LLM. At least one element must contain `{prompt}`. | — |
| `context`     | no       | Standing text sent with every question. | `--context`, `--no-context` |
| `aliases`     | no       | Extra selectors, for shorter typing. | — |
| `neutral_dir` | no       | Run in an empty temporary directory. Defaults to true. | `--here`, `--neutral-dir` |

A table is what an LLM does by default; the flags in the last column change it for one run, which is how a script gets what it needs without a second table.

Where the context goes depends on the command. If any element contains `{context}`, the context is substituted there, which is how you reach a flag like `--append-system-prompt`. If no element does, the context is prepended to the question with a blank line between, which works for any LLM that takes only a bare prompt.

`neutral_dir` exists because some LLM CLIs walk up from the working directory looking for project instructions. A one-shot terminal question should not drag in whatever repository you happen to be standing in, so by default the LLM runs in an empty temporary directory that is removed when it exits. Turn it off for an LLM whose configuration lives in the directory you are in, or override it either way with `--here` and `--neutral-dir`.

Configuration errors are caught before anything is spawned: a `default` that names no table, a `command` with no `{prompt}`, a `{context}` with no context set, an alias that collides with another selector, or a misspelled key all fail with a message naming the file and the LLM.

## License

MIT. See [LICENSE](LICENSE).
