//! Shell completion scripts for the `issue` command.
//!
//! `issue completions <shell>` prints a script to stdout. The scripts complete
//! subcommands and flags statically, and call back into the binary's hidden
//! `__complete-ids` / `__complete-labels` helpers for dynamic values (existing
//! issue ids, labels). Still std-only — these are just string constants.

/// Returns the completion script for `shell`, or `None` if unsupported.
pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

/// The shells we can emit, for help text and error messages.
pub const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

const ZSH: &str = r#"#compdef issue
# issue(1) zsh completion.
# Install (pick one):
#   issue completions zsh > "${fpath[1]}/_issue"   # then restart zsh
#   echo 'source <(issue completions zsh)' >> ~/.zshrc
_issue() {
  local -a subcmds
  subcmds=(
    'init:Initialize the issue directory'
    'create:Create an issue'
    'list:List issues'
    'view:Show a single issue'
    'edit:Edit an issue in place'
    'close:Close an issue'
    'reopen:Reopen an issue'
    'lint:Detect duplicate ids'
    'export:Export issues as GitHub JSON'
    'import:Import issues from GitHub JSON'
    'completions:Print a shell completion script'
  )
  if (( CURRENT == 2 )); then
    _describe -t commands 'issue command' subcmds
    return
  fi
  local cmd=${words[2]}
  local prev=${words[CURRENT-1]}
  case $prev in
    --status)
      _values 'status' 'open' 'closed'; return ;;
    --label|--add-label|--remove-label)
      local -a labels; labels=(${(f)"$(issue __complete-labels 2>/dev/null)"})
      _describe -t labels 'label' labels; return ;;
  esac
  case $cmd in
    view|close|reopen)
      local -a ids; ids=(${(f)"$(issue __complete-ids 2>/dev/null)"})
      _describe -t ids 'issue' ids ;;
    edit)
      if [[ ${words[CURRENT]} == -* ]]; then
        _values 'flag' '--title' '--status' '--add-label' '--remove-label' '--body'
      else
        local -a ids; ids=(${(f)"$(issue __complete-ids 2>/dev/null)"})
        _describe -t ids 'issue' ids
      fi ;;
    create) _values 'flag' '--title' '--label' '--status' '--body' ;;
    list) _values 'flag' '--status' '--label' ;;
    completions) _values 'shell' 'bash' 'zsh' 'fish' ;;
  esac
}
if [ "$funcstack[1]" = "_issue" ]; then
  _issue "$@"
else
  compdef _issue issue
fi
"#;

const BASH: &str = r#"# issue(1) bash completion.
# Install (pick one):
#   issue completions bash > /usr/local/etc/bash_completion.d/issue
#   echo 'source <(issue completions bash)' >> ~/.bashrc
_issue() {
  local cur prev cmd
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  case "$prev" in
    --status)
      COMPREPLY=( $(compgen -W "open closed" -- "$cur") ); return ;;
    --label|--add-label|--remove-label)
      COMPREPLY=( $(compgen -W "$(issue __complete-labels 2>/dev/null)" -- "$cur") ); return ;;
  esac
  if [ "$COMP_CWORD" -eq 1 ]; then
    COMPREPLY=( $(compgen -W "init create list view edit close reopen lint export import completions" -- "$cur") )
    return
  fi
  cmd="${COMP_WORDS[1]}"
  case "$cmd" in
    view|close|reopen)
      COMPREPLY=( $(compgen -W "$(issue __complete-ids 2>/dev/null | cut -d: -f1)" -- "$cur") ) ;;
    edit)
      if [[ "$cur" == -* ]]; then
        COMPREPLY=( $(compgen -W "--title --status --add-label --remove-label --body" -- "$cur") )
      else
        COMPREPLY=( $(compgen -W "$(issue __complete-ids 2>/dev/null | cut -d: -f1)" -- "$cur") )
      fi ;;
    create) COMPREPLY=( $(compgen -W "--title --label --status --body" -- "$cur") ) ;;
    list) COMPREPLY=( $(compgen -W "--status --label" -- "$cur") ) ;;
    completions) COMPREPLY=( $(compgen -W "bash zsh fish" -- "$cur") ) ;;
  esac
}
complete -F _issue issue
"#;

const FISH: &str = r#"# issue(1) fish completion.
# Install: issue completions fish > ~/.config/fish/completions/issue.fish
complete -c issue -f

# Subcommands (only when no subcommand is present yet).
complete -c issue -n __fish_use_subcommand -a init -d 'Initialize the issue directory'
complete -c issue -n __fish_use_subcommand -a create -d 'Create an issue'
complete -c issue -n __fish_use_subcommand -a list -d 'List issues'
complete -c issue -n __fish_use_subcommand -a view -d 'Show a single issue'
complete -c issue -n __fish_use_subcommand -a edit -d 'Edit an issue in place'
complete -c issue -n __fish_use_subcommand -a close -d 'Close an issue'
complete -c issue -n __fish_use_subcommand -a reopen -d 'Reopen an issue'
complete -c issue -n __fish_use_subcommand -a lint -d 'Detect duplicate ids'
complete -c issue -n __fish_use_subcommand -a export -d 'Export issues as GitHub JSON'
complete -c issue -n __fish_use_subcommand -a import -d 'Import issues from GitHub JSON'
complete -c issue -n __fish_use_subcommand -a completions -d 'Print a shell completion script'

# Issue ids (id<TAB>title) for commands that take an <id>.
complete -c issue -n '__fish_seen_subcommand_from view close reopen edit' \
  -a '(issue __complete-ids 2>/dev/null | string replace -r ":" \t)'

# Flags per subcommand.
complete -c issue -n '__fish_seen_subcommand_from create' -l title -l label -l status -l body
complete -c issue -n '__fish_seen_subcommand_from edit' -l title -l status -l add-label -l remove-label -l body
complete -c issue -n '__fish_seen_subcommand_from list' -l status -l label
complete -c issue -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish'

# Values for --status / --label.
complete -c issue -n '__fish_seen_subcommand_from create edit list' -l status -a 'open closed'
complete -c issue -n '__fish_seen_subcommand_from create edit list import' -l label \
  -a '(issue __complete-labels 2>/dev/null)'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_shells_have_scripts() {
        for sh in SHELLS {
            assert!(script(sh).is_some(), "missing script for {sh}");
        }
        assert!(script("powershell").is_none());
    }

    #[test]
    fn scripts_mention_every_subcommand() {
        // Drift guard: if a subcommand is added, it must appear in each script.
        let cmds = [
            "init", "create", "list", "view", "edit", "close", "reopen", "lint", "export",
            "import", "completions",
        ];
        for sh in SHELLS {
            let s = script(sh).unwrap();
            for c in cmds {
                assert!(s.contains(c), "{sh} script missing subcommand {c}");
            }
        }
    }

    #[test]
    fn scripts_use_dynamic_helpers() {
        for sh in SHELLS {
            let s = script(sh).unwrap();
            assert!(s.contains("__complete-ids"), "{sh} missing id completion");
            assert!(s.contains("__complete-labels"), "{sh} missing label completion");
        }
    }
}
