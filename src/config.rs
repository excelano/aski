//! Loading and validating `config.toml`.
//!
//! The file names one `[llm.<selector>]` table per command-line LLM plus a
//! top-level `default`. Everything a run needs — the argv to spawn, the standing
//! context, whether to run in a neutral directory — lives in that table, so the
//! binary carries no per-LLM knowledge of its own.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Substituted with the question. Required somewhere in every `command`.
pub const PROMPT: &str = "{prompt}";
/// Substituted with the LLM's `context`. Optional — see [`Llm::context`].
pub const CONTEXT: &str = "{context}";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Selector used when the first argument matches no configured LLM.
    pub default: String,
    #[serde(default)]
    pub llm: BTreeMap<String, Llm>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Llm {
    /// argv for the LLM, spawned directly with no shell in between. At least one
    /// element must contain `{prompt}`; any element may contain `{context}`.
    pub command: Vec<String>,
    /// Standing context sent with every question. Goes wherever `{context}`
    /// appears in `command`; if it appears nowhere, it is prepended to the
    /// question instead.
    #[serde(default)]
    pub context: String,
    /// Extra selectors for this LLM, for shorter typing.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Run the LLM in an empty temporary directory. On by default: a one-shot
    /// question should not pick up the project you happen to be standing in.
    #[serde(default = "enabled")]
    pub neutral_dir: bool,
}

fn enabled() -> bool {
    true
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = fs::read_to_string(path).map_err(|e| {
            format!(
                "cannot read {}: {e}\nrun `aski --init` to write a starter config",
                path.display()
            )
        })?;
        let config: Config =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        config
            .validate()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(config)
    }

    /// The LLM a first argument selects, or `None` if it selects nothing and is
    /// therefore the start of the question.
    pub fn select(&self, selector: &str) -> Option<(&str, &Llm)> {
        if let Some((name, llm)) = self.llm.get_key_value(selector) {
            return Some((name.as_str(), llm));
        }
        self.llm
            .iter()
            .find(|(_, llm)| llm.aliases.iter().any(|a| a == selector))
            .map(|(name, llm)| (name.as_str(), llm))
    }

    /// The LLM used when nothing was selected. Validation guarantees it exists.
    pub fn default_llm(&self) -> (&str, &Llm) {
        let (name, llm) = self
            .llm
            .get_key_value(&self.default)
            .expect("validated default");
        (name.as_str(), llm)
    }

    fn validate(&self) -> Result<(), String> {
        if self.llm.is_empty() {
            return Err("no [llm.*] tables configured".into());
        }
        if !self.llm.contains_key(&self.default) {
            return Err(format!(
                "default = \"{}\" names no configured llm",
                self.default
            ));
        }

        // One selector namespace: an alias cannot collide with a table name or
        // with another alias, or the same argument would mean two things. Names
        // go in first -- all of them -- so a later table cannot claim a selector
        // an earlier table's alias already took.
        let mut selectors: BTreeMap<&str, &str> = self
            .llm
            .keys()
            .map(|name| (name.as_str(), name.as_str()))
            .collect();
        for (name, llm) in &self.llm {
            for alias in &llm.aliases {
                match selectors.entry(alias.as_str()) {
                    Entry::Occupied(taken) => {
                        return Err(format!(
                            "alias \"{alias}\" on llm \"{name}\" is already a selector for \"{}\"",
                            taken.get()
                        ));
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(name.as_str());
                    }
                }
            }
        }

        for (name, llm) in &self.llm {
            llm.validate().map_err(|e| format!("llm \"{name}\": {e}"))?;
        }
        Ok(())
    }
}

impl Llm {
    /// The argv to spawn, with both placeholders filled in.
    pub fn argv(&self, prompt: &str) -> Vec<String> {
        self.command
            .iter()
            .map(|arg| arg.replace(PROMPT, prompt).replace(CONTEXT, &self.context))
            .collect()
    }

    /// Whether `context` is carried by the argv rather than by the prompt text.
    pub fn context_in_command(&self) -> bool {
        self.command.iter().any(|arg| arg.contains(CONTEXT))
    }

    fn validate(&self) -> Result<(), String> {
        if self.command.is_empty() {
            return Err("command is empty".into());
        }
        if !self.command.iter().any(|arg| arg.contains(PROMPT)) {
            return Err(format!("command has no {PROMPT} placeholder"));
        }
        if self.context_in_command() && self.context.is_empty() {
            return Err(format!("command uses {CONTEXT} but context is not set"));
        }
        Ok(())
    }
}

/// `$ASKI_CONFIG`, else `$XDG_CONFIG_HOME/aski/config.toml`, else
/// `~/.config/aski/config.toml`.
pub fn path() -> Result<PathBuf, String> {
    if let Some(override_path) = env::var_os("ASKI_CONFIG")
        && !override_path.is_empty()
    {
        return Ok(PathBuf::from(override_path));
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|dir| !dir.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or("cannot locate a config directory: set HOME or ASKI_CONFIG")?;
    Ok(base.join("aski").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    const TWO: &str = r#"
        default = "alpha"
        [llm.alpha]
        command = ["a", "--sys", "{context}", "{prompt}"]
        context = "be terse"
        aliases = ["a1"]
        [llm.beta]
        command = ["b", "{prompt}"]
    "#;

    #[test]
    fn selects_by_name_and_by_alias() {
        let config = parse(TWO).unwrap();
        assert_eq!(config.select("alpha").unwrap().0, "alpha");
        assert_eq!(config.select("a1").unwrap().0, "alpha");
        assert_eq!(config.select("beta").unwrap().0, "beta");
        assert!(config.select("how").is_none());
        assert_eq!(config.default_llm().0, "alpha");
    }

    #[test]
    fn substitutes_both_placeholders() {
        let config = parse(TWO).unwrap();
        let alpha = config.select("alpha").unwrap().1;
        assert_eq!(
            alpha.argv("why"),
            ["a", "--sys", "be terse", "why"].map(String::from)
        );
        assert!(alpha.context_in_command());

        let beta = config.select("beta").unwrap().1;
        assert_eq!(beta.argv("why"), ["b", "why"].map(String::from));
        assert!(!beta.context_in_command());
    }

    #[test]
    fn substitutes_a_placeholder_embedded_in_an_argument() {
        let config = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "Question: {prompt}"]
        "#,
        )
        .unwrap();
        assert_eq!(
            config.default_llm().1.argv("why"),
            ["a", "Question: why"].map(String::from)
        );
    }

    #[test]
    fn neutral_dir_is_on_unless_turned_off() {
        let config = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "{prompt}"]
            [llm.b]
            command = ["b", "{prompt}"]
            neutral_dir = false
        "#,
        )
        .unwrap();
        assert!(config.select("a").unwrap().1.neutral_dir);
        assert!(!config.select("b").unwrap().1.neutral_dir);
    }

    #[test]
    fn rejects_a_default_naming_no_llm() {
        let err = parse(
            r#"
            default = "zeta"
            [llm.a]
            command = ["a", "{prompt}"]
        "#,
        )
        .unwrap_err();
        assert!(err.contains("names no configured llm"), "{err}");
    }

    #[test]
    fn rejects_a_command_without_a_prompt_placeholder() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "--sys", "{context}"]
            context = "be terse"
        "#,
        )
        .unwrap_err();
        assert!(err.contains("no {prompt} placeholder"), "{err}");
    }

    #[test]
    fn rejects_a_context_placeholder_with_no_context() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "--sys", "{context}", "{prompt}"]
        "#,
        )
        .unwrap_err();
        assert!(err.contains("but context is not set"), "{err}");
    }

    #[test]
    fn rejects_an_empty_command() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = []
        "#,
        )
        .unwrap_err();
        assert!(err.contains("command is empty"), "{err}");
    }

    #[test]
    fn rejects_no_llms_at_all() {
        let err = parse(r#"default = "a""#).unwrap_err();
        assert!(err.contains("no [llm.*] tables"), "{err}");
    }

    #[test]
    fn rejects_an_alias_that_collides_with_a_later_table_name() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "{prompt}"]
            aliases = ["b"]
            [llm.b]
            command = ["b", "{prompt}"]
        "#,
        )
        .unwrap_err();
        assert!(err.contains(r#"alias "b" on llm "a""#), "{err}");
    }

    #[test]
    fn rejects_an_alias_shared_by_two_llms() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "{prompt}"]
            aliases = ["q"]
            [llm.b]
            command = ["b", "{prompt}"]
            aliases = ["q"]
        "#,
        )
        .unwrap_err();
        assert!(err.contains(r#"alias "q" on llm "b""#), "{err}");
    }

    #[test]
    fn rejects_an_unknown_key() {
        let err = parse(
            r#"
            default = "a"
            [llm.a]
            command = ["a", "{prompt}"]
            widget = 1
        "#,
        )
        .unwrap_err();
        assert!(err.contains("unknown field `widget`"), "{err}");
    }

    #[test]
    fn the_shipped_starter_config_is_valid() {
        let config = parse(include_str!("../starter.toml")).unwrap();
        assert_eq!(config.default_llm().0, "claude");
    }
}
