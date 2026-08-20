//! aski — a shortcut to each LLM's one-shot CLI mode.
//!
//! The point is that a question typed at the shell should not need quoting, so
//! argument parsing is deliberately shallow: the first argument may select an
//! LLM or be a flag, and every argument after that is question text, whatever it
//! looks like.

mod config;

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use config::{Config, Llm};

const STARTER: &str = include_str!("../starter.toml");

const USAGE: &str = "\
aski - one-shot questions for command-line LLMs (no follow-up)

Usage:
  aski <question>                ask the default LLM, no quotes needed
  aski <llm> <question>          ask a configured LLM by name or alias
  aski -- <question>             ask the default LLM, even if the question
                                 starts with an LLM's name
  command | aski <question>      pipe context in on stdin

Options (first argument only, so a flag inside a question stays text):
  -h, --help                     show this help
  -V, --version                  show the version
      --list                     list the configured LLMs
      --init                     write a starter config file

Quote the question only when it contains shell metacharacters.
For a conversation, run the LLM's own CLI instead.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("aski: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        Some("-V" | "--version") => {
            println!("aski {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Some("--list") => return list(),
        Some("--init") => return init(),
        _ => {}
    }

    let path = config::path()?;
    let config = Config::load(&path)?;

    // The first argument selects an LLM only if it names one. Anything else --
    // including a bare `--`, which is the escape hatch for a question that
    // starts with an LLM's name -- leaves the default in place.
    let (llm, question) = match args.split_first() {
        Some((first, rest)) if first == "--" => (config.default_llm().1, rest),
        Some((first, rest)) => match config.select(first) {
            Some((_, llm)) => (llm, rest),
            None => (config.default_llm().1, args.as_slice()),
        },
        None => (config.default_llm().1, args.as_slice()),
    };

    let prompt = build_prompt(question, llm)?;
    spawn(llm, &prompt)
}

/// The question as typed, plus anything piped in, plus the context when the
/// command has no `{context}` placeholder to carry it.
fn build_prompt(question: &[String], llm: &Llm) -> Result<String, String> {
    let question = question.join(" ");

    let mut piped = String::new();
    if !std::io::stdin().is_terminal() {
        std::io::stdin()
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

    if !llm.context.is_empty() && !llm.context_in_command() {
        prompt = format!("{}\n\n{prompt}", llm.context.trim());
    }
    Ok(prompt)
}

fn spawn(llm: &Llm, prompt: &str) -> Result<ExitCode, String> {
    let argv = llm.argv(prompt);
    let (program, rest) = argv.split_first().expect("validated command");

    let mut command = Command::new(program);
    command.args(rest);

    // Once stdin has been drained into the prompt, hand the LLM nothing rather
    // than an exhausted handle it might sit and wait on.
    if !std::io::stdin().is_terminal() {
        command.stdin(Stdio::null());
    }

    // Held for the child's lifetime; dropping it removes the directory.
    let neutral_dir = if llm.neutral_dir {
        let dir = tempfile::Builder::new()
            .prefix("aski-")
            .tempdir()
            .map_err(|e| format!("cannot create a neutral directory: {e}"))?;
        command.current_dir(dir.path());
        Some(dir)
    } else {
        None
    };

    let status = command
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    drop(neutral_dir);

    Ok(match status.code() {
        Some(code) => ExitCode::from(code as u8),
        None => ExitCode::FAILURE,
    })
}

fn list() -> Result<ExitCode, String> {
    let path = config::path()?;
    let config = Config::load(&path)?;

    println!("{}", path.display());
    for (name, llm) in &config.llm {
        let marker = if name == &config.default { "*" } else { " " };
        let selectors = std::iter::once(name.clone())
            .chain(llm.aliases.iter().cloned())
            .collect::<Vec<_>>()
            .join(", ");
        println!("{marker} {selectors}\n    {}", llm.command.join(" "));
    }
    println!("\n* default");
    Ok(ExitCode::SUCCESS)
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
    println!("wrote {}", path.display());
    Ok(ExitCode::SUCCESS)
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
