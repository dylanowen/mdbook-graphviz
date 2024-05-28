use anyhow::anyhow;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{de, Deserialize, Deserializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum D2Error {
    #[error("Parse Error: {0:?}")]
    Parse(ParseError),
    #[error("{0}")]
    D2(String),
    #[error("Internal Error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl D2Error {
    pub fn from_error_string(error_string: &str) -> Self {
        match serde_json::from_str::<RawD2Error>(error_string) {
            Ok(raw_error) => {
                if let Some(parse_error) = raw_error.parse_error {
                    D2Error::Parse(parse_error)
                } else {
                    D2Error::D2(raw_error.message)
                }
            }
            Err(error) => D2Error::Internal(anyhow!(
                "Failed to parse Error {error}: {}",
                error_string.to_string()
            )),
        }
    }
}

#[derive(Deserialize, Debug)]
struct RawD2Error {
    message: String,
    parse_error: Option<ParseError>,
}

#[derive(Deserialize, Debug)]
pub struct ParseError {
    #[serde(alias = "errs")]
    pub errors: Vec<AstError>,
}

#[derive(Deserialize, Debug)]
pub struct AstError {
    #[serde(deserialize_with = "string_to_range")]
    pub range: Range,
    #[serde(alias = "errmsg")]
    pub message: String,
}

#[derive(Debug)]
pub struct Range {
    pub path: String,
    pub start: Position,
    pub end: Position,
}

#[derive(Debug)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub byte: usize,
}

fn string_to_range<'de, D>(de: D) -> Result<Range, D::Error>
where
    D: Deserializer<'de>,
{
    lazy_static! {
        static ref RANGE_RE: Regex = Regex::new(r"^([^,]*),([\d:]+)-([\d:]+)$").unwrap();
    }

    let raw_range = String::deserialize(de)?;

    let (path, start, end) = RANGE_RE
        .captures(&raw_range)
        .and_then(|parsed_range| {
            Some((
                parsed_range.get(1)?.as_str(),
                parsed_range.get(2)?.as_str(),
                parsed_range.get(3)?.as_str(),
            ))
        })
        .ok_or(de::Error::custom("Invalid Range String"))?;

    Ok(Range {
        path: path.to_string(),
        start: string_to_position(start).map_err(de::Error::custom)?,
        end: string_to_position(end).map_err(de::Error::custom)?,
    })
}

fn string_to_position(raw_position: &str) -> Result<Position, String> {
    lazy_static! {
        static ref POSITION_RE: Regex = Regex::new(r"^(\d+):(\d+):(\d+)$").unwrap();
    }

    let (line, column, byte) = POSITION_RE
        .captures(raw_position)
        .and_then(|parsed_range| {
            Some((
                parsed_range.get(1)?.as_str(),
                parsed_range.get(2)?.as_str(),
                parsed_range.get(3)?.as_str(),
            ))
        })
        .ok_or("Invalid Position String".to_string())?;

    Ok(Position {
        line: line
            .parse::<usize>()
            .map_err(|e| format!("Not a number: {e:?}"))?,
        column: column
            .parse::<usize>()
            .map_err(|e| format!("Not a number: {e:?}"))?,
        byte: byte
            .parse::<usize>()
            .map_err(|e| format!("Not a number: {e:?}"))?,
    })
}

#[cfg(test)]
mod test {
    use crate::d2_sys::render;

    #[test]
    fn test_parse_error() {
        let err = render(r#"Chicken's plan: {"#).unwrap_err();

        println!("{:?}", err);
    }
}
