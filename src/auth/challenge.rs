use super::invalid;
use crate::error::Error;
use reqwest::header::HeaderMap;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Challenge {
    pub metadata: Option<String>,
    pub scope: Option<String>,
}

pub(super) fn parse(headers: &HeaderMap) -> Result<Challenge, Error> {
    let mut result: Option<Challenge> = None;
    for header in headers.get_all("www-authenticate") {
        let value: &str = header.to_str().map_err(|_| invalid())?;
        if value.len() > 16384 {
            return Err(invalid());
        }
        for (scheme, fields) in challenges(value)? {
            if !scheme.eq_ignore_ascii_case("bearer") {
                continue;
            }
            if result.is_some() {
                return Err(invalid());
            }
            result = Some(Challenge {
                metadata: fields.get("resource_metadata").cloned(),
                scope: fields.get("scope").cloned(),
            });
        }
    }
    Ok(result.unwrap_or_default())
}

type Fields = BTreeMap<String, String>;

fn challenges(mut input: &str) -> Result<Vec<(String, Fields)>, Error> {
    let mut output: Vec<(String, Fields)> = Vec::new();
    while !input.trim().is_empty() {
        input = input.trim_start_matches([' ', '\t', ',']);
        if input.is_empty() {
            break;
        }
        let (word, rest): (&str, &str) = token(input)?;
        input = rest.trim_start();
        if let Some(rest) = input.strip_prefix('=') {
            let fields: &mut Fields = &mut output.last_mut().ok_or_else(invalid)?.1;
            let (value, rest): (String, &str) = parameter(rest.trim_start())?;
            if fields.insert(word.to_ascii_lowercase(), value).is_some() {
                return Err(invalid());
            }
            input = rest.trim_start();
            if !input.is_empty() && !input.starts_with(',') {
                return Err(invalid());
            }
        } else {
            output.push((word.to_owned(), Fields::new()));
            if !word.eq_ignore_ascii_case("bearer") {
                input = skip_token68(input).unwrap_or(input);
            }
        }
    }
    Ok(output)
}

fn skip_token68(input: &str) -> Option<&str> {
    let length = input.find([',', ' ', '\t']).unwrap_or(input.len());
    let token: &str = input[..length].trim_end_matches('=');
    (!token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte)))
    .then_some(&input[length..])
}

fn token(input: &str) -> Result<(&str, &str), Error> {
    let length = input
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte))
        .count();
    if length == 0 {
        return Err(invalid());
    }
    Ok(input.split_at(length))
}

fn parameter(input: &str) -> Result<(String, &str), Error> {
    if !input.starts_with('"') {
        let (value, rest): (&str, &str) = token(input)?;
        return Ok((value.to_owned(), rest));
    }
    let mut value: String = String::new();
    let mut escaped = false;
    for (index, character) in input[1..].char_indices() {
        if character.is_control() {
            return Err(invalid());
        }
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok((value, &input[index + 2..]));
        } else {
            value.push(character);
        }
    }
    Err(invalid())
}

pub(super) fn scopes(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.split(' ').any(|scope| {
            scope.is_empty()
                || !scope.bytes().all(|byte| {
                    byte == 0x21 || (0x23..=0x5b).contains(&byte) || (0x5d..=0x7e).contains(&byte)
                })
        })
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn union(first: Option<&str>, second: Option<&str>) -> Result<Option<String>, Error> {
    let mut values: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for value in [first, second].into_iter().flatten() {
        scopes(value)?;
        values.extend(value.split(' '));
    }
    if values.is_empty() {
        return Ok(None);
    }
    let value: String = values.into_iter().collect::<Vec<&str>>().join(" ");
    if value.len() > 16384 {
        return Err(invalid());
    }
    Ok(Some(value))
}
