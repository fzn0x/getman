//! Variables and `{{name}}` interpolation.
//!
//! Variables come from several scopes. When two scopes define the same
//! name, the more specific one wins, in this order from weakest to
//! strongest:
//!
//! 1. globals
//! 2. the workspace (`getman.yaml`)
//! 3. the selected environment
//! 4. folders, outermost first
//! 5. the request
//! 6. the current row of a data file, in a run
//! 7. local variables set by scripts with `gm.variables.set`
//!
//! Besides variables, `{{$name}}` produces a dynamic value such as a UUID
//! or a timestamp (see [`crate::dynamic`]), and `{{$env.GETMAN_NAME}}`
//! reads an environment variable of the process. Only names starting with
//! `GETMAN_` are readable, so a collection from someone else cannot read
//! the other secrets in your shell.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::dynamic;
use crate::model::Vars;

/// Prefix an operating-system environment variable needs to be readable
/// through `{{$env.NAME}}`.
pub const ENV_PREFIX: &str = "GETMAN_";

const MAX_DEPTH: usize = 10;

/// Where a variable is defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Global variables.
    Global,
    /// Workspace variables.
    Workspace,
    /// The selected environment.
    Environment,
    /// A folder enclosing the request.
    Folder,
    /// The request itself.
    Request,
    /// The current row of a data file.
    Data,
    /// Set by a script for the rest of the run.
    Local,
}

/// Variables from every scope, consulted from the strongest down.
#[derive(Debug, Clone, Default)]
pub struct VarStack {
    layers: Vec<(Scope, Vars)>,
}

impl VarStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a layer that takes precedence over every layer before it.
    pub fn push(&mut self, scope: Scope, vars: Vars) {
        if !vars.is_empty() {
            self.layers.push((scope, vars));
        }
    }

    /// The value of a variable, if any scope defines it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.lookup(name).map(|(_, value)| value)
    }

    /// The value of a variable and the scope it came from.
    pub fn lookup(&self, name: &str) -> Option<(Scope, &str)> {
        self.layers
            .iter()
            .rev()
            .find_map(|(scope, vars)| vars.get(name).map(|value| (*scope, value.as_str())))
    }

    /// Replaces a scope's variable in the strongest layer of that scope, or
    /// adds a layer for it.
    pub fn set(&mut self, scope: Scope, name: &str, value: &str) {
        if let Some((_, vars)) = self.layers.iter_mut().rev().find(|(s, _)| *s == scope) {
            vars.insert(name.to_owned(), value.to_owned());
        } else {
            let vars = std::iter::once((name.to_owned(), value.to_owned())).collect();
            let at = self
                .layers
                .iter()
                .position(|(s, _)| *s > scope)
                .unwrap_or(self.layers.len());
            self.layers.insert(at, (scope, vars));
        }
    }

    /// Removes a variable from every layer of a scope.
    pub fn unset(&mut self, scope: Scope, name: &str) {
        for (_, vars) in self.layers.iter_mut().filter(|(s, _)| *s == scope) {
            vars.shift_remove(name);
        }
    }

    /// Every variable with the value that wins, ordered by name.
    pub fn resolved(&self) -> Vec<ResolvedVar> {
        let names: BTreeSet<&str> = self
            .layers
            .iter()
            .flat_map(|(_, vars)| vars.keys().map(String::as_str))
            .collect();
        names
            .into_iter()
            .filter_map(|name| {
                self.lookup(name).map(|(scope, value)| ResolvedVar {
                    name: name.to_owned(),
                    value: value.to_owned(),
                    scope,
                })
            })
            .collect()
    }

    /// Replaces every `{{name}}` in `text`.
    pub fn interpolate(&self, text: &str) -> Interpolated {
        let mut missing = BTreeSet::new();
        let text = self.expand(text, 0, &mut Vec::new(), &mut missing);
        Interpolated {
            text,
            missing: missing.into_iter().collect(),
        }
    }

    /// Replaces every `{{name}}` in `text`, ignoring any that are missing.
    pub fn render(&self, text: &str) -> String {
        self.interpolate(text).text
    }

    fn expand(
        &self,
        text: &str,
        depth: usize,
        stack: &mut Vec<String>,
        missing: &mut BTreeSet<String>,
    ) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                out.push_str(&rest[start..]);
                return out;
            };
            let raw = &after[..end];
            let name = raw.trim();
            if !is_reference(name) {
                // Not a variable, such as `{{` in a template or JSON; keep
                // the braces and look for a reference after them.
                out.push_str("{{");
                rest = after;
                continue;
            }
            match self.value_of(name) {
                Some(_) if stack.iter().any(|seen| seen == name) || depth >= MAX_DEPTH => {
                    // A variable that refers to itself, directly or not.
                    missing.insert(name.to_owned());
                    out.push_str("{{");
                    out.push_str(raw);
                    out.push_str("}}");
                }
                Some(value) => {
                    stack.push(name.to_owned());
                    let expanded = self.expand(&value, depth + 1, stack, missing);
                    stack.pop();
                    out.push_str(&expanded);
                }
                None => {
                    missing.insert(name.to_owned());
                    out.push_str("{{");
                    out.push_str(raw);
                    out.push_str("}}");
                }
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        out
    }

    fn value_of(&self, name: &str) -> Option<String> {
        if let Some(env_name) = name.strip_prefix("$env.") {
            return read_process_env(env_name);
        }
        if let Some(dynamic_name) = name.strip_prefix('$') {
            return dynamic::generate(dynamic_name);
        }
        self.get(name).map(str::to_owned)
    }
}

/// A variable's winning value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedVar {
    /// Variable name.
    pub name: String,
    /// The value that wins.
    pub value: String,
    /// Scope the value comes from.
    pub scope: Scope,
}

/// Text after interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interpolated {
    /// The text with every known variable replaced.
    pub text: String,
    /// Names that were referenced but not defined, left as `{{name}}`.
    pub missing: Vec<String>,
}

/// Whether the text between `{{` and `}}` names a variable.
fn is_reference(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && !name.contains(['{', '}', '\n', '\r', '"'])
}

pub(crate) fn read_process_env(name: &str) -> Option<String> {
    if !name.starts_with(ENV_PREFIX) {
        return None;
    }
    std::env::var(name).ok()
}

/// Says why `{{name}}` was left as written. A `$env.` name without the
/// [`ENV_PREFIX`] was refused rather than unset, and the message says so,
/// since the fix is to rename the variable, not to set it.
pub fn missing_reason(name: &str) -> String {
    match name.strip_prefix("$env.") {
        Some(env) if !env.starts_with(ENV_PREFIX) => format!(
            "{{{{{name}}}}} was not read: only environment variables named {ENV_PREFIX}* can be read"
        ),
        Some(_) => format!("{{{{{name}}}}} is not set in the environment"),
        None => format!("{{{{{name}}}}} is not defined"),
    }
}

/// Names of the variables `text` refers to, without dynamic values.
pub fn references(text: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if is_reference(name) && !name.starts_with('$') {
            names.insert(name.to_owned());
            rest = &after[end + 2..];
        } else {
            rest = after;
        }
    }
    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vars {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn stronger_scopes_win() {
        let mut stack = VarStack::new();
        stack.push(Scope::Workspace, vars(&[("host", "ws"), ("only", "ws")]));
        stack.push(Scope::Environment, vars(&[("host", "env")]));
        stack.push(Scope::Request, vars(&[("host", "req")]));
        assert_eq!(stack.lookup("host"), Some((Scope::Request, "req")));
        assert_eq!(stack.lookup("only"), Some((Scope::Workspace, "ws")));
        assert_eq!(stack.get("nope"), None);
    }

    #[test]
    fn set_adds_a_layer_in_precedence_order() {
        let mut stack = VarStack::new();
        stack.push(Scope::Request, vars(&[("x", "req")]));
        stack.set(Scope::Environment, "x", "env");
        assert_eq!(stack.get("x"), Some("req"));
        stack.set(Scope::Local, "x", "local");
        assert_eq!(stack.get("x"), Some("local"));
        stack.unset(Scope::Local, "x");
        assert_eq!(stack.get("x"), Some("req"));
    }

    #[test]
    fn interpolation_replaces_nested_references() {
        let mut stack = VarStack::new();
        stack.push(
            Scope::Environment,
            vars(&[
                ("base", "https://{{host}}:{{ port }}"),
                ("host", "api.example.com"),
                ("port", "8443"),
            ]),
        );
        let out = stack.interpolate("{{base}}/users?q={{missing}}");
        assert_eq!(out.text, "https://api.example.com:8443/users?q={{missing}}");
        assert_eq!(out.missing, ["missing"]);
    }

    #[test]
    fn cycles_are_left_unresolved() {
        let mut stack = VarStack::new();
        stack.push(Scope::Workspace, vars(&[("a", "<{{b}}>"), ("b", "{{a}}")]));
        let out = stack.interpolate("{{a}}");
        assert_eq!(out.text, "<{{a}}>");
        assert_eq!(out.missing, ["a"]);
    }

    #[test]
    fn braces_that_are_not_references_are_kept() {
        let stack = VarStack::new();
        assert_eq!(stack.render(r#"{{"a": {{}}}}"#), r#"{{"a": {{}}}}"#);
        assert_eq!(stack.render("open {{ only"), "open {{ only");
    }

    #[test]
    fn dynamic_values_differ_per_reference() {
        let stack = VarStack::new();
        let out = stack.render("{{$uuid}} {{$uuid}}");
        let (a, b) = out.split_once(' ').unwrap();
        assert_eq!(a.len(), 36);
        assert_ne!(a, b);
    }

    #[test]
    fn only_prefixed_process_variables_are_readable() {
        let stack = VarStack::new();
        assert!(stack.render("{{$env.PATH}}").contains("{{$env.PATH}}"));
        let out = stack.interpolate("{{$env.GETMAN_SURELY_UNSET_FOR_TESTS}}");
        assert_eq!(out.missing, ["$env.GETMAN_SURELY_UNSET_FOR_TESTS"]);
        assert_eq!(
            missing_reason("$env.GETMAN_SURELY_UNSET_FOR_TESTS"),
            "{{$env.GETMAN_SURELY_UNSET_FOR_TESTS}} is not set in the environment"
        );
        assert_eq!(
            missing_reason("$env.PATH"),
            "{{$env.PATH}} was not read: only environment variables named GETMAN_* can be read"
        );
        assert_eq!(missing_reason("token"), "{{token}} is not defined");
    }

    #[test]
    fn references_lists_variable_names() {
        assert_eq!(
            references("{{base}}/x/{{ id }}?t={{$timestamp}}&a={{base}}"),
            ["base", "id"]
        );
    }

    #[test]
    fn resolved_lists_winning_values() {
        let mut stack = VarStack::new();
        stack.push(Scope::Global, vars(&[("a", "1"), ("b", "2")]));
        stack.push(Scope::Local, vars(&[("a", "3")]));
        let resolved = stack.resolved();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].value, "3");
        assert_eq!(resolved[0].scope, Scope::Local);
    }
}
