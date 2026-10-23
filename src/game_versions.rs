use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::state::AppState;

pub const SETTING_KEY: &str = "game_swf";
pub const VERSIONS_SUBDIR: &str = "versions";

fn is_version_file(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".swf") else { return false };
    let mut chars = stem.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn label(file: &str) -> String {
    file.strip_prefix("Base_sec_")
        .and_then(|rest| rest.strip_suffix(".swf"))
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| file.trim_end_matches(".swf").to_string())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Token {
    Number(u64),
    Text(String),
}

fn natural_key(name: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    let flush_digits = |digits: &mut String, tokens: &mut Vec<Token>| {
        if !digits.is_empty() {
            tokens.push(Token::Number(digits.parse().unwrap_or(u64::MAX)));
            digits.clear();
        }
    };
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            if !text.is_empty() {
                tokens.push(Token::Text(std::mem::take(&mut text)));
            }
            digits.push(ch);
        } else {
            flush_digits(&mut digits, &mut tokens);
            text.push(ch);
        }
    }
    flush_digits(&mut digits, &mut tokens);
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    tokens
}

pub struct Version {
    pub file: String,
    pub version: String,
}

pub async fn available_versions(state: &AppState) -> Vec<Version> {
    let directory = state.settings.flash_dir.join(VERSIONS_SUBDIR);
    let Ok(mut entries) = tokio::fs::read_dir(&directory).await else { return Vec::new() };
    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_file = entry.file_type().await.map(|t| t.is_file()).unwrap_or(false);
        if is_file && is_version_file(&name) {
            files.push(name);
        }
    }
    files.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)));
    files.into_iter().map(|file| Version { version: label(&file), file }).collect()
}

pub fn default_version(state: &AppState) -> String {
    state.settings.swf_version.clone()
}

pub async fn selected_version(state: &AppState) -> ApiResult<String> {
    if let Some(stored) = state.accounts.get_setting(SETTING_KEY).await? {
        if available_versions(state).await.iter().any(|v| v.file == stored) {
            return Ok(stored);
        }
    }
    Ok(default_version(state))
}

pub async fn select_version(state: &AppState, file: &str) -> ApiResult<bool> {
    if !available_versions(state).await.iter().any(|v| v.file == file) {
        return Ok(false);
    }
    state.accounts.set_setting(SETTING_KEY, file).await?;
    Ok(true)
}

pub async fn payload(state: &AppState) -> ApiResult<Value> {
    let versions: Vec<Value> = available_versions(state)
        .await
        .into_iter()
        .map(|v| json!({ "file": v.file, "version": v.version }))
        .collect();
    Ok(json!({
        "selected": selected_version(state).await?,
        "default": default_version(state),
        "versions": versions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_swf_names_are_versions() {
        assert!(is_version_file("Base_sec_0.6.7c.swf"));
        assert!(!is_version_file("Base_sec_0.6.7c - Flashpoint.swf"));
        assert!(!is_version_file("../secret.swf"));
        assert!(!is_version_file("notes.txt"));
        assert!(!is_version_file(".swf"));
    }

    #[test]
    fn versions_sort_numerically_and_get_a_label() {
        let mut names = vec!["Base_sec_0.10.0.swf", "Base_sec_0.6.7c.swf", "Base_sec_0.7.9.swf"];
        names.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)));
        assert_eq!(names, ["Base_sec_0.6.7c.swf", "Base_sec_0.7.9.swf", "Base_sec_0.10.0.swf"]);
        assert_eq!(label("Base_sec_0.6.7c.swf"), "0.6.7c");
        assert_eq!(label("Other.swf"), "Other");
    }
}
