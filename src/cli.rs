use clap::{Parser, Subcommand, ValueEnum};

use crate::error::NanError;
use crate::model::{NativeLanguage, ProficiencyLevel};

#[derive(Debug, Parser)]
#[command(name = "nan")]
#[command(version)]
#[command(about = "Natural Japanese learning with sentence-first review")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Add {
        sentence: String,
        style: Option<String>,
    },
    New {
        first: Option<String>,
        second: Option<String>,
    },
    Cat {
        n: Option<usize>,
    },
    List {
        #[arg(allow_hyphen_values = true)]
        first: Option<String>,
        #[arg(allow_hyphen_values = true)]
        second: Option<String>,
    },
    Del {
        n: usize,
    },
    Set {
        key: SetKey,
        option: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ListTarget {
    Word,
    #[default]
    Sentence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SetKey {
    Ref,
    Level,
    #[value(name = "base-url")]
    BaseUrl,
    #[value(name = "api-key")]
    ApiKey,
    Model,
    Roomaji,
    Furigana,
    Lan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArgs {
    pub count: usize,
    pub style: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListArgs {
    pub count: Option<isize>,
    pub target: ListTarget,
}

impl Command {
    pub fn resolve_new_args(&self) -> Result<Option<NewArgs>, NanError> {
        match self {
            Self::New { first, second } => {
                Ok(Some(resolve_new_args(first.as_deref(), second.as_deref())?))
            }
            _ => Ok(None),
        }
    }

    pub fn resolve_list_args(&self) -> Result<Option<ListArgs>, NanError> {
        match self {
            Self::List { first, second } => Ok(Some(resolve_list_args(
                first.as_deref(),
                second.as_deref(),
            )?)),
            _ => Ok(None),
        }
    }
}

pub fn resolve_new_args(first: Option<&str>, second: Option<&str>) -> Result<NewArgs, NanError> {
    let default_count = 1;

    match (first, second) {
        (None, None) => Ok(NewArgs {
            count: default_count,
            style: None,
        }),
        (Some(first), None) => {
            if let Ok(count) = first.parse::<usize>() {
                if count == 0 {
                    return Err(NanError::message("new count must be greater than 0"));
                }

                return Ok(NewArgs { count, style: None });
            }

            Ok(NewArgs {
                count: default_count,
                style: Some(first.to_string()),
            })
        }
        (Some(first), Some(second)) => {
            let count = first.parse::<usize>().map_err(|_| {
                NanError::message(
                    "when two arguments are provided to `nan new`, the first argument must be a positive integer",
                )
            })?;

            if count == 0 {
                return Err(NanError::message("new count must be greater than 0"));
            }

            Ok(NewArgs {
                count,
                style: Some(second.to_string()),
            })
        }
        (None, Some(_)) => Err(NanError::message(
            "internal argument state for `nan new` is invalid",
        )),
    }
}

pub fn resolve_list_args(first: Option<&str>, second: Option<&str>) -> Result<ListArgs, NanError> {
    match (first, second) {
        (None, None) => Ok(ListArgs {
            count: None,
            target: ListTarget::Sentence,
        }),
        (Some(first), None) => {
            if let Some(target) = parse_list_target(first) {
                return Ok(ListArgs {
                    count: None,
                    target,
                });
            }

            let count = parse_list_count(first)?;
            Ok(ListArgs {
                count: Some(count),
                target: ListTarget::Sentence,
            })
        }
        (Some(first), Some(second)) => {
            let count = parse_list_count(first)?;
            let target = parse_list_target(second).ok_or_else(|| {
                NanError::message("list target must be either `word` or `sentence`")
            })?;
            Ok(ListArgs {
                count: Some(count),
                target,
            })
        }
        (None, Some(_)) => Err(NanError::message(
            "internal argument state for `nan list` is invalid",
        )),
    }
}

fn parse_list_count(value: &str) -> Result<isize, NanError> {
    let count = value
        .parse::<isize>()
        .map_err(|_| NanError::message(format!("invalid list count: `{value}`")))?;
    if count == 0 {
        return Err(NanError::message("list count must not be 0"));
    }
    Ok(count)
}

fn parse_list_target(value: &str) -> Option<ListTarget> {
    match value {
        "word" => Some(ListTarget::Word),
        "sentence" => Some(ListTarget::Sentence),
        _ => None,
    }
}

pub fn parse_native_language(option: &str) -> Result<NativeLanguage, NanError> {
    match option {
        "english" => Ok(NativeLanguage::English),
        "chinese" => Ok(NativeLanguage::Chinese),
        _ => Err(NanError::message(
            "lan must be either `english` or `chinese`",
        )),
    }
}

pub fn parse_proficiency_level(option: &str) -> Result<ProficiencyLevel, NanError> {
    match option {
        "n5.5" => Ok(ProficiencyLevel::N55),
        "n5" => Ok(ProficiencyLevel::N5),
        "n4.5" => Ok(ProficiencyLevel::N45),
        "n4" => Ok(ProficiencyLevel::N4),
        "n3.5" => Ok(ProficiencyLevel::N35),
        "n3" => Ok(ProficiencyLevel::N3),
        "n2.5" => Ok(ProficiencyLevel::N25),
        "n2" => Ok(ProficiencyLevel::N2),
        "n1.5" => Ok(ProficiencyLevel::N15),
        "n1" => Ok(ProficiencyLevel::N1),
        _ => Err(NanError::message(
            "level must be one of n5.5/n5/n4.5/n4/n3.5/n3/n2.5/n2/n1.5/n1",
        )),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, ListArgs, ListTarget, resolve_list_args, resolve_new_args};

    #[test]
    fn new_defaults_to_single_sentence() {
        let args = resolve_new_args(None, None).expect("default new args should resolve");
        assert_eq!(args.count, 1);
        assert_eq!(args.style, None);
    }

    #[test]
    fn new_treats_integer_as_count() {
        let args = resolve_new_args(Some("3"), None).expect("count should parse");
        assert_eq!(args.count, 3);
        assert_eq!(args.style, None);
    }

    #[test]
    fn new_treats_non_integer_as_style() {
        let args = resolve_new_args(Some("daily"), None).expect("style should parse");
        assert_eq!(args.count, 1);
        assert_eq!(args.style.as_deref(), Some("daily"));
    }

    #[test]
    fn new_requires_integer_when_two_args_are_provided() {
        let error = resolve_new_args(Some("daily"), Some("soft")).expect_err("should fail");
        assert!(
            error
                .to_string()
                .contains("the first argument must be a positive integer")
        );
    }

    #[test]
    fn list_defaults_to_sentence_target() {
        let cli = Cli::parse_from(["nan", "list"]);
        let args = cli
            .command
            .resolve_list_args()
            .expect("list args should resolve")
            .expect("expected list args");
        assert_eq!(args.target, ListTarget::Sentence);
        assert_eq!(args.count, None);
    }

    #[test]
    fn list_word_without_count_is_valid() {
        let args = resolve_list_args(Some("word"), None).expect("list args should resolve");
        assert_eq!(
            args,
            ListArgs {
                count: None,
                target: ListTarget::Word,
            }
        );
    }

    #[test]
    fn list_accepts_negative_values_without_double_dash() {
        let cli = Cli::parse_from(["nan", "list", "-2", "sentence"]);
        let args = cli
            .command
            .resolve_list_args()
            .expect("list args should resolve")
            .expect("expected list args");
        assert_eq!(args.count, Some(-2));
        assert_eq!(args.target, ListTarget::Sentence);
    }

    #[test]
    fn list_word_is_not_parsed_as_count() {
        let cli = Cli::parse_from(["nan", "list", "word"]);
        let args = cli
            .command
            .resolve_list_args()
            .expect("list args should resolve")
            .expect("expected list args");
        assert_eq!(args.count, None);
        assert_eq!(args.target, ListTarget::Word);
    }

    #[test]
    fn list_rejects_zero_count() {
        let error = resolve_list_args(Some("0"), None).expect_err("zero count should fail");
        assert!(error.to_string().contains("list count must not be 0"));
    }

    #[test]
    fn list_rejects_unknown_target() {
        let error =
            resolve_list_args(Some("3"), Some("words")).expect_err("bad target should fail");
        assert!(
            error
                .to_string()
                .contains("list target must be either `word` or `sentence`")
        );
    }
}
