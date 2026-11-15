use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalAddress {
    pub machine: Option<String>,
    pub target: String,
}

impl TerminalAddress {
    pub fn parse(raw: &str) -> Self {
        if let Some((machine, target)) = raw.split_once('/') {
            if !machine.is_empty() && !target.is_empty() {
                return Self {
                    machine: Some(machine.to_string()),
                    target: target.to_string(),
                };
            }
        }
        Self {
            machine: None,
            target: raw.to_string(),
        }
    }

    pub fn is_remote(&self) -> bool {
        self.machine.is_some()
    }
}

impl fmt::Display for TerminalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.machine {
            Some(m) => write!(f, "{m}/{}", self.target),
            None => write!(f, "{}", self.target),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_local_and_remote_addresses() {
        let local = TerminalAddress::parse("t7");
        assert_eq!(local.machine, None);
        assert_eq!(local.target, "t7");
        assert!(!local.is_remote());

        let named = TerminalAddress::parse("api-agent");
        assert_eq!(named.machine, None);
        assert_eq!(named.target, "api-agent");

        let remote = TerminalAddress::parse("rohan/t7");
        assert_eq!(remote.machine.as_deref(), Some("rohan"));
        assert_eq!(remote.target, "t7");
        assert!(remote.is_remote());
        assert_eq!(remote.to_string(), "rohan/t7");
    }
}
