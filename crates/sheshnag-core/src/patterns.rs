//! Detection patterns. They run on a plain command string (arguments joined by spaces),
//! so inline interpreters and `/tmp/` paths match however the arguments were stored.
use regex::Regex;
use std::sync::OnceLock;

fn common_hiders() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"base64|osascript|curl|wget|/tmp/|/private/tmp|nohup|python[0-9.]* -c|perl -e|eval ").unwrap()
    })
}

fn miners() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)xmrig|minerd|kdevtmpfsi|cpuminer|nicehash|xmr-stak|lolminer|t-rex|nbminer|rigupdater|--donate-level|stratum\+tcp",
        )
        .unwrap()
    })
}

fn common_pasted() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"curl[^|]*\|[ ]*(ba)?sh|base64 (-d|--decode)|osascript").unwrap())
}

/// Tokens that suggest a command hides what it does. Sorted, unique, each followed by a space,
/// which is the format of the original script's messages.
pub fn hidden_tokens(command: &str, platform_extra: &str) -> Option<String> {
    let mut found: Vec<String> = common_hiders().find_iter(command).map(|m| m.as_str().to_owned()).collect();
    if !platform_extra.is_empty() {
        if let Ok(extra) = Regex::new(&format!("(?i){platform_extra}")) {
            found.extend(extra.find_iter(command).map(|m| m.as_str().to_owned()));
        }
    }
    if found.is_empty() {
        return None;
    }
    found.sort();
    found.dedup();
    Some(found.iter().map(|t| format!("{t} ")).collect())
}

pub fn is_miner(comm: &str, args: &str) -> bool {
    miners().is_match(&format!("{comm} {args}"))
}

/// True when a shell start-up line looks like a pasted download-and-run command.
pub fn is_pasted_command(line: &str, platform_extra: &str) -> bool {
    if common_pasted().is_match(line) {
        return true;
    }
    !platform_extra.is_empty()
        && Regex::new(&format!("(?i){platform_extra}")).map(|r| r.is_match(line)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_python_matches_however_arguments_were_stored() {
        assert!(hidden_tokens("python3 -c print(1)", "").is_some());
    }

    #[test]
    fn tmp_path_matches_in_a_plain_command() {
        assert_eq!(hidden_tokens("/tmp/payload", "").unwrap(), "/tmp/ ");
    }

    #[test]
    fn ordinary_program_is_clean() {
        assert!(hidden_tokens("/usr/bin/true", "").is_none());
    }

    #[test]
    fn tokens_are_sorted_unique_and_space_terminated() {
        assert_eq!(hidden_tokens("curl x | base64 -d; curl y", "").unwrap(), "base64 curl ");
    }

    #[test]
    fn platform_extra_is_case_insensitive() {
        assert!(hidden_tokens("POWERSHELL -EncodedCommand AAA", r"-encodedcommand").is_some());
    }

    #[test]
    fn miners_match_by_name_ignoring_case() {
        assert!(is_miner("/tmp/XMRig", "x"));
        assert!(!is_miner("/usr/bin/true", "true"));
    }

    #[test]
    fn pasted_curl_pipe_shell_is_detected() {
        assert!(is_pasted_command("curl http://x | sh", ""));
        assert!(!is_pasted_command("export A=1", ""));
    }
}
