//! aski — a shortcut to each LLM's one-shot CLI mode.
//!
//! The point is that a question typed at the shell should not need quoting, so
//! argument parsing is deliberately shallow: flags are read only while they
//! lead, and every argument from the first non-flag onward is question text,
//! whatever it looks like.

mod args;
mod config;

use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::Path;
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use args::{Ask, Context, Mode};
use config::{CONTEXT, Config, Llm};

const STARTER: &str = include_str!("../starter.toml");

/// The LLM's own code passes through, so aski's results sit in the band that
/// `timeout(1)` and `env(1)` reserve for a wrapper's own troubles.
const TIMED_OUT: u8 = 124;
const ASKI_FAILED: u8 = 125;
const NOT_RUNNABLE: u8 = 126;
const NOT_FOUND: u8 = 127;

const USAGE: &str = "\
aski - one-shot questions for command-line LLMs (no follow-up)

Usage:
  aski <question>                ask the default LLM, no quotes needed
  aski <llm> <question>          ask a configured LLM by name or alias
  aski [options] -- <question>   ask without the first word selecting an LLM
  command | aski <question>      pipe context in on stdin

Options lead. Parsing stops at the first word that is not a flag, so a flag
inside a question stays text and `aski what does --help do` asks.

  -h, --help                     show this help
  -V, --version                  show the version
      --list                     list the configured LLMs
      --init                     write a starter config file

  -l, --llm <name>               ask this LLM, failing if it is not configured
      --context <text>           send this standing context instead of the
                                 configured one
      --no-context               send the question with no standing context
      --timeout <duration>       kill the LLM after 30, 90s, 2m, 1h
      --here                     run the LLM in the current directory
      --neutral-dir              run the LLM in an empty temporary directory
      --no-stdin                 read no stdin, and give the LLM none
  -n, --dry-run                  print the command that would run, and stop

Environment:
  ASKI_LLM                       selector used in place of the config default
  ASKI_CONFIG                    path to the config file

Exit codes:
  0-123                          the LLM's own exit code, passed through
  124                            the LLM was killed by --timeout
  125                            aski itself failed
  126                            the LLM was found but could not be run
  127                            the LLM's command was not found
  128+n                          the LLM was killed by signal n

Quote the question only when it contains shell metacharacters.
For a conversation, run the LLM's own CLI instead.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("aski: {message}");
            ExitCode::from(ASKI_FAILED)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    match args::parse(&argv)? {
        Mode::Help => Ok(say(&format!("{USAGE}\n"))),
        Mode::Version => Ok(say(&format!("aski {}\n", env!("CARGO_PKG_VERSION")))),
        Mode::List => list(),
        Mode::Init => init(),
        Mode::Ask(ask) => ask_llm(ask),
    }
}

fn ask_llm(ask: Ask) -> Result<ExitCode, String> {
    let path = config::path()?;
    let config = Config::load(&path)?;

    // Naming an LLM is a claim that it exists, so a name that matches nothing
    // fails here rather than quietly becoming the first word of the question.
    let from_env = env_llm();
    let named = match (ask.llm.as_deref(), from_env.as_deref()) {
        (Some(selector), _) => Some(("--llm", selector)),
        (None, Some(selector)) => Some(("ASKI_LLM", selector)),
        (None, None) => None,
    };
    let fallback = match named {
        Some((source, selector)) => match config.select(selector) {
            Some((_, llm)) => llm,
            None => return Err(format!("{source}: \"{selector}\" names no configured llm")),
        },
        None => config.default_llm().1,
    };

    // Only an unclaimed first word can still select an LLM, and only when it
    // names one; otherwise it is where the question starts.
    let (llm, question) = match ask.words.split_first() {
        Some((first, rest)) if ask.positional_llm => match config.select(first) {
            Some((_, llm)) => (llm, rest),
            None => (fallback, ask.words.as_slice()),
        },
        _ => (fallback, ask.words.as_slice()),
    };

    let context = match &ask.context {
        Context::Configured => llm.context.as_str(),
        Context::Replaced(text) => text.as_str(),
        Context::Suppressed => "",
    };
    if context.trim().is_empty() && llm.context_in_command() {
        return Err(format!(
            "this llm's command carries the context in {CONTEXT}, so it cannot run without one"
        ));
    }

    let prompt = build_prompt(question, context, llm, ask.read_stdin)?;
    let argv = llm.argv(&prompt, context);

    if ask.dry_run {
        let line = argv
            .iter()
            .map(|arg| quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        return Ok(say(&format!("{line}\n")));
    }
    spawn(&argv, llm, &ask)
}

fn env_llm() -> Option<String> {
    std::env::var("ASKI_LLM")
        .ok()
        .filter(|name| !name.is_empty())
}

/// The question as typed, plus anything piped in, plus the context when the
/// command has no `{context}` placeholder to carry it.
fn build_prompt(
    question: &[String],
    context: &str,
    llm: &Llm,
    read_stdin: bool,
) -> Result<String, String> {
    let question = question.join(" ");

    let mut piped = String::new();
    if read_stdin && !io::stdin().is_terminal() {
        io::stdin()
            .read_to_string(&mut piped)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
    }
    let piped = piped.trim();

    let mut prompt = match (question.is_empty(), piped.is_empty()) {
        (true, true) => {
            eprintln!("{USAGE}");
            return Err("no question".into());
        }
        (true, false) => piped.to_string(),
        (false, true) => question,
        (false, false) => format!("{question}\n\n{piped}"),
    };

    if !context.trim().is_empty() && !llm.context_in_command() {
        prompt = format!("{}\n\n{prompt}", context.trim());
    }
    Ok(prompt)
}

fn spawn(argv: &[String], llm: &Llm, ask: &Ask) -> Result<ExitCode, String> {
    let (program, rest) = argv.split_first().expect("validated command");

    let mut command = Command::new(program);
    command.args(rest);

    // Either stdin has been drained into the prompt or it was refused. Hand the
    // LLM nothing rather than an exhausted handle it might sit and wait on.
    if !ask.read_stdin || !io::stdin().is_terminal() {
        command.stdin(Stdio::null());
    }

    // Held for the child's lifetime; dropping it removes the directory.
    let neutral_dir = if ask.neutral_dir.unwrap_or(llm.neutral_dir) {
        let dir = tempfile::Builder::new()
            .prefix("aski-")
            .tempdir()
            .map_err(|e| format!("cannot create a neutral directory: {e}"))?;
        command.current_dir(dir.path());
        Some(dir)
    } else {
        None
    };

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("aski: cannot run {program}: {e}");
            return Ok(ExitCode::from(match e.kind() {
                io::ErrorKind::NotFound => NOT_FOUND,
                io::ErrorKind::PermissionDenied => NOT_RUNNABLE,
                _ => ASKI_FAILED,
            }));
        }
    };

    let status =
        wait(&mut child, ask.timeout).map_err(|e| format!("cannot wait for {program}: {e}"))?;
    drop(neutral_dir);

    Ok(match status {
        Some(status) => exit_code(status),
        None => {
            eprintln!("aski: {program} did not answer in time");
            ExitCode::from(TIMED_OUT)
        }
    })
}

/// Waits for the child, returning `None` once it has been killed for running
/// past the limit. Polling keeps this to the standard library; the interval
/// backs off so a fast answer is not held up and a slow one costs nothing.
fn wait(child: &mut Child, timeout: Option<Duration>) -> io::Result<Option<ExitStatus>> {
    let Some(limit) = timeout else {
        return child.wait().map(Some);
    };

    let start = Instant::now();
    let mut nap = Duration::from_millis(2);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let left = limit.saturating_sub(start.elapsed());
        if left.is_zero() {
            let _ = child.kill();
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(nap.min(left));
        nap = (nap * 2).min(Duration::from_millis(100));
    }
}

fn exit_code(status: ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        return ExitCode::from(code as u8);
    }
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return ExitCode::from(128u8.saturating_add(signal as u8));
    }
    ExitCode::from(ASKI_FAILED)
}

/// Writes to stdout, treating a closed pipe as the end of the job rather than
/// as a panic, so `aski --list | head` is not a crash.
fn say(text: &str) -> ExitCode {
    use std::io::Write;

    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aski: cannot write to stdout: {e}");
            ExitCode::from(ASKI_FAILED)
        }
    }
}

/// Single-quoted for a shell, so `--dry-run` prints a line that can be pasted.
fn quote(arg: &str) -> String {
    const SAFE: &[u8] = b"-_./:=@,+";
    if !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || SAFE.contains(&b))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

fn list() -> Result<ExitCode, String> {
    use std::fmt::Write;

    let path = config::path()?;
    let config = Config::load(&path)?;

    let mut out = format!("{}\n", path.display());
    for (name, llm) in &config.llm {
        let marker = if name == &config.default { "*" } else { " " };
        let selectors = std::iter::once(name.clone())
            .chain(llm.aliases.iter().cloned())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "{marker} {selectors}\n    {}", llm.command.join(" "));
    }
    out.push_str("\n* default\n");
    Ok(say(&out))
}

fn init() -> Result<ExitCode, String> {
    let path = config::path()?;
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    write_new(&path, STARTER)?;
    Ok(say(&format!("wrote {}\n", path.display())))
}

/// Write only if nothing is there, so `--init` can never clobber a config that
/// appeared between the check above and now.
fn write_new(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;

    let mut file =
        fs::File::create_new(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_what_a_shell_would_reinterpret() {
        assert_eq!(quote("--model"), "--model");
        assert_eq!(quote("be terse"), "'be terse'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("don't"), r"'don'\''t'");
    }
}
