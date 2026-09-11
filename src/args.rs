//! Argument parsing: a leading run of flags, then question text.
//!
//! Flags are read only while they lead. Parsing stops at the first argument
//! that is not a flag, or at `--`, and everything from there on is question
//! text whatever it looks like — which is what lets `aski what does --help do`
//! ask rather than print usage.

use std::time::Duration;

/// What the command line asked for.
#[derive(Debug, PartialEq)]
pub enum Mode {
    Help,
    Version,
    List,
    Init,
    Ask(Ask),
}

/// A question, plus the per-run overrides that let a script name what it wants
/// instead of relying on what the config happens to hold.
#[derive(Debug, PartialEq)]
pub struct Ask {
    /// Selector from `-l`/`--llm`. An unknown one is an error, never question text.
    pub llm: Option<String>,
    /// Whether the first word may still select an LLM. Off once `-l` or `--` is seen.
    pub positional_llm: bool,
    pub context: Context,
    pub timeout: Option<Duration>,
    /// Overrides the LLM's `neutral_dir`.
    pub neutral_dir: Option<bool>,
    pub dry_run: bool,
    pub read_stdin: bool,
    pub words: Vec<String>,
}

/// Where the standing context for this run comes from.
#[derive(Debug, PartialEq)]
pub enum Context {
    /// Whatever the LLM's table configures.
    Configured,
    Replaced(String),
    Suppressed,
}

impl Default for Ask {
    fn default() -> Ask {
        Ask {
            llm: None,
            positional_llm: true,
            context: Context::Configured,
            timeout: None,
            neutral_dir: None,
            dry_run: false,
            read_stdin: true,
            words: Vec::new(),
        }
    }
}

pub fn parse(args: &[String]) -> Result<Mode, String> {
    let mut ask = Ask::default();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" {
            ask.positional_llm = false;
            i += 1;
            break;
        }
        if !is_flag(arg) {
            break;
        }

        let (name, inline) = split(arg);
        match name {
            "-h" | "--help" => return solo(name, Mode::Help, args),
            "-V" | "--version" => return solo(name, Mode::Version, args),
            "--list" => return solo(name, Mode::List, args),
            "--init" => return solo(name, Mode::Init, args),
            "-l" | "--llm" => ask.llm = Some(value(name, inline, args, &mut i)?),
            "--context" => ask.context = Context::Replaced(value(name, inline, args, &mut i)?),
            "--timeout" => ask.timeout = Some(duration(&value(name, inline, args, &mut i)?)?),
            "--no-context" => {
                bare(name, inline)?;
                ask.context = Context::Suppressed;
            }
            "--here" => {
                bare(name, inline)?;
                ask.neutral_dir = Some(false);
            }
            "--neutral-dir" => {
                bare(name, inline)?;
                ask.neutral_dir = Some(true);
            }
            "--no-stdin" => {
                bare(name, inline)?;
                ask.read_stdin = false;
            }
            "-n" | "--dry-run" => {
                bare(name, inline)?;
                ask.dry_run = true;
            }
            _ => {
                return Err(format!(
                    "unknown option {name}\n\
                     to ask a question that starts with a dash, put `--` first"
                ));
            }
        }
        i += 1;
    }

    if ask.llm.is_some() {
        ask.positional_llm = false;
    }
    ask.words = args[i..].to_vec();
    Ok(Mode::Ask(ask))
}

/// A lone `-` is not a flag: nothing here reads a file, so it stays question text.
fn is_flag(arg: &str) -> bool {
    arg.len() > 1 && arg.starts_with('-')
}

/// `--name=value` splits; a short flag's value is always the next argument.
fn split(arg: &str) -> (&str, Option<&str>) {
    match arg.split_once('=') {
        Some((name, value)) if name.starts_with("--") => (name, Some(value)),
        _ => (arg, None),
    }
}

/// A mode flag answers the whole command line, so anything beside it is a
/// question the run would otherwise drop on the floor.
fn solo(name: &str, mode: Mode, args: &[String]) -> Result<Mode, String> {
    if args.len() == 1 {
        return Ok(mode);
    }
    Err(format!(
        "{name} takes no other arguments\n\
         to ask a question containing it, put `--` first"
    ))
}

fn value(
    name: &str,
    inline: Option<&str>,
    args: &[String],
    i: &mut usize,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value.to_string());
    }
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{name} needs a value"))
}

fn bare(name: &str, inline: Option<&str>) -> Result<(), String> {
    match inline {
        Some(_) => Err(format!("{name} takes no value")),
        None => Ok(()),
    }
}

/// Bare seconds, or a `s`/`m`/`h` suffix, the way `timeout(1)` spells them.
fn duration(text: &str) -> Result<Duration, String> {
    let bad = || format!("--timeout: \"{text}\" is not a duration (try 30, 90s, 2m, or 1h)");
    let (digits, scale) = match text.as_bytes().last() {
        Some(b's') => (&text[..text.len() - 1], 1.0),
        Some(b'm') => (&text[..text.len() - 1], 60.0),
        Some(b'h') => (&text[..text.len() - 1], 3600.0),
        _ => (text, 1.0),
    };
    let seconds: f64 = digits.parse::<f64>().map_err(|_| bad())? * scale;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 31_536_000.0 {
        return Err(bad());
    }
    Ok(Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ask(line: &str) -> Ask {
        let args: Vec<String> = line.split(' ').map(String::from).collect();
        match parse(&args).unwrap() {
            Mode::Ask(ask) => ask,
            other => panic!("expected a question, got {other:?}"),
        }
    }

    fn err(line: &str) -> String {
        let args: Vec<String> = line.split(' ').map(String::from).collect();
        parse(&args).unwrap_err()
    }

    fn words(ask: &Ask) -> String {
        ask.words.join(" ")
    }

    #[test]
    fn a_flag_after_the_question_has_started_is_question_text() {
        let ask = parse_ask("what does --help do");
        assert_eq!(words(&ask), "what does --help do");
        assert!(ask.positional_llm);
        assert!(!ask.dry_run);
    }

    #[test]
    fn leading_flags_are_read_until_the_question_starts() {
        let ask = parse_ask("-n --llm gemini --timeout 2m why is -n useful");
        assert_eq!(ask.llm.as_deref(), Some("gemini"));
        assert_eq!(ask.timeout, Some(Duration::from_secs(120)));
        assert!(ask.dry_run);
        assert_eq!(words(&ask), "why is -n useful");
    }

    #[test]
    fn naming_an_llm_stops_the_first_word_from_selecting_one() {
        assert!(!parse_ask("-l claude claude keeps asking").positional_llm);
        assert!(!parse_ask("-- claude keeps asking").positional_llm);
        assert_eq!(
            words(&parse_ask("-- claude keeps asking")),
            "claude keeps asking"
        );
    }

    #[test]
    fn a_value_may_be_attached_with_an_equals_sign() {
        let ask = parse_ask("--llm=gemini --context=be+terse why");
        assert_eq!(ask.llm.as_deref(), Some("gemini"));
        assert_eq!(ask.context, Context::Replaced("be+terse".into()));
    }

    #[test]
    fn a_dash_terminator_ends_flags_even_before_a_flag() {
        let ask = parse_ask("-- --timeout is a flag of what");
        assert_eq!(words(&ask), "--timeout is a flag of what");
        assert_eq!(ask.timeout, None);
    }

    #[test]
    fn a_lone_dash_is_question_text() {
        let ask = parse_ask("- means stdin where");
        assert_eq!(words(&ask), "- means stdin where");
    }

    #[test]
    fn context_can_be_replaced_or_suppressed() {
        assert_eq!(parse_ask("--no-context why").context, Context::Suppressed);
        assert_eq!(
            parse_ask("--context terse why").context,
            Context::Replaced("terse".into())
        );
        assert_eq!(parse_ask("why").context, Context::Configured);
    }

    #[test]
    fn the_working_directory_can_be_forced_either_way() {
        assert_eq!(parse_ask("--here why").neutral_dir, Some(false));
        assert_eq!(parse_ask("--neutral-dir why").neutral_dir, Some(true));
        assert_eq!(parse_ask("why").neutral_dir, None);
    }

    #[test]
    fn stdin_is_read_unless_it_is_refused() {
        assert!(parse_ask("why").read_stdin);
        assert!(!parse_ask("--no-stdin why").read_stdin);
    }

    #[test]
    fn mode_flags_stand_alone() {
        let args = vec!["--list".to_string()];
        assert_eq!(parse(&args).unwrap(), Mode::List);
        assert!(err("--help me with this").contains("takes no other arguments"));
    }

    #[test]
    fn rejects_an_unknown_leading_flag_rather_than_asking_about_it() {
        let message = err("--depth means what");
        assert!(message.contains("unknown option --depth"), "{message}");
        assert!(message.contains("`--` first"), "{message}");
    }

    #[test]
    fn rejects_a_flag_with_a_missing_or_surplus_value() {
        assert!(err("--llm").contains("--llm needs a value"));
        assert!(err("--no-stdin=yes why").contains("--no-stdin takes no value"));
    }

    #[test]
    fn reads_the_durations_timeout_reads() {
        assert_eq!(
            parse_ask("--timeout 30 why").timeout,
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_ask("--timeout 90s why").timeout,
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            parse_ask("--timeout 1h why").timeout,
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            parse_ask("--timeout 0.5 why").timeout,
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn rejects_a_duration_that_is_not_one() {
        for bad in [
            "--timeout soon why",
            "--timeout 0 why",
            "--timeout -5 why",
            "--timeout s why",
        ] {
            assert!(err(bad).contains("is not a duration"), "{bad}");
        }
    }

    #[test]
    fn an_empty_command_line_is_a_question_with_no_words() {
        let ask = parse_ask("");
        assert_eq!(ask.words, [""]);
        let empty: Vec<String> = Vec::new();
        assert_eq!(parse(&empty).unwrap(), Mode::Ask(Ask::default()));
    }
}
