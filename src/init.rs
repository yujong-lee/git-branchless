//! Install any hooks, aliases, etc. to set up `git-branchless` in this repo.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use console::style;
use fn_error_context::context;
use log::warn;
use pyo3::prelude::*;

use crate::config::get_core_hooks_path;
use crate::python::{map_err_to_py_err, TextIO};
use crate::util::{get_repo, run_git_silent, wrap_git_error, GitExecutable, GitVersion};

#[derive(Debug)]
enum Hook {
    /// Regular Git hook.
    RegularHook { path: PathBuf },

    /// For Twitter multihooks.
    MultiHook { path: PathBuf },
}

#[context("Determining hook path")]
fn determine_hook_path(repo: &git2::Repository, hook_type: &str) -> anyhow::Result<Hook> {
    let multi_hooks_path = repo.path().join("hook_multi");
    let hook = if multi_hooks_path.exists() {
        let path = multi_hooks_path
            .join(format!("{}.d", hook_type))
            .join("00_local_branchless");
        Hook::MultiHook { path }
    } else {
        let hooks_dir = get_core_hooks_path(repo)?;
        let path = hooks_dir.join(hook_type);
        Hook::RegularHook { path }
    };
    Ok(hook)
}

const SHEBANG: &str = "#!/bin/sh";
const UPDATE_MARKER_START: &str = "## START BRANCHLESS CONFIG";
const UPDATE_MARKER_END: &str = "## END BRANCHLESS CONFIG";

fn update_between_lines(lines: &str, updated_lines: &str) -> String {
    let mut new_lines = String::new();
    let mut is_ignoring_lines = false;
    for line in lines.lines() {
        if line == UPDATE_MARKER_START {
            is_ignoring_lines = true;
            new_lines.push_str(UPDATE_MARKER_START);
            new_lines.push('\n');
            new_lines.push_str(updated_lines);
            new_lines.push_str(UPDATE_MARKER_END);
            new_lines.push('\n');
        } else if line == UPDATE_MARKER_END {
            is_ignoring_lines = false;
        } else if !is_ignoring_lines {
            new_lines.push_str(line);
            new_lines.push('\n');
        }
    }
    if is_ignoring_lines {
        warn!("Unterminated branchless config comment in hook");
    }
    new_lines
}

#[context("Updating hook contents: {:?}", hook)]
fn update_hook_contents(hook: &Hook, hook_contents: &str) -> anyhow::Result<()> {
    let (hook_path, hook_contents) = match hook {
        Hook::RegularHook { path } => match std::fs::read_to_string(path) {
            Ok(lines) => {
                let lines = update_between_lines(&lines, hook_contents);
                (path, lines)
            }
            Err(ref err) if err.kind() == std::io::ErrorKind::NotFound => {
                let hook_contents = format!(
                    "{}\n{}\n{}\n{}\n",
                    SHEBANG, UPDATE_MARKER_START, hook_contents, UPDATE_MARKER_END
                );
                (path, hook_contents)
            }
            Err(other) => {
                return Err(anyhow::anyhow!(other));
            }
        },
        Hook::MultiHook { path } => (path, hook_contents.to_owned()),
    };

    let hook_dir = hook_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No parent for dir {:?}", hook_path))?;
    std::fs::create_dir_all(hook_dir)
        .with_context(|| format!("Creating hook dir {:?}", hook_path))?;
    std::fs::write(hook_path, hook_contents)
        .with_context(|| format!("Writing hook contents to {:?}", hook_path))?;

    // Setting hook file as executable only supported on Unix systems.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(hook_path)
            .with_context(|| format!("Reading hook permissions for {:?}", hook_path))?;
        let mut permissions = metadata.permissions();
        let mode = permissions.mode();
        // Set execute bits.
        let mode = mode | 0o111;
        permissions.set_mode(mode);
        std::fs::set_permissions(hook_path, permissions)
            .with_context(|| format!("Marking {:?} as executable", hook_path))?;
    }

    Ok(())
}

#[context("Installing hook of type: {:?}", hook_type)]
fn install_hook<Out: Write>(
    out: &mut Out,
    repo: &git2::Repository,
    hook_type: &str,
    hook_script: &str,
) -> anyhow::Result<()> {
    writeln!(out, "Installing hook: {}", hook_type)?;
    let hook = determine_hook_path(repo, hook_type)?;
    update_hook_contents(&hook, hook_script)?;
    Ok(())
}

#[context("Installing all hooks")]
fn install_hooks<Out: Write>(out: &mut Out, repo: &git2::Repository) -> anyhow::Result<()> {
    install_hook(
        out,
        repo,
        "post-commit",
        r#"
git branchless hook-post-commit "$@"
"#,
    )?;
    install_hook(
        out,
        repo,
        "post-rewrite",
        r#"
git branchless hook-post-rewrite "$@"
"#,
    )?;
    install_hook(
        out,
        repo,
        "post-checkout",
        r#"
git branchless hook-post-checkout "$@"
"#,
    )?;
    install_hook(
        out,
        repo,
        "pre-auto-gc",
        r#"
git branchless hook-pre-auto-gc "$@"
"#,
    )?;
    install_hook(
        out,
        repo,
        "reference-transaction",
        r#"
# Avoid canceling the reference transaction in the case that `branchless` fails
# for whatever reason.
git branchless hook-reference-transaction "$@" || (
    echo 'branchless: Failed to process reference transaction!'
    echo 'branchless: Some events (e.g. branch updates) may have been lost.'
    echo 'branchless: This is a bug. Please report it.'
)
"#,
    )?;
    Ok(())
}

#[context("Installing alias: {:?}", alias)]
fn install_alias<Out: Write>(
    out: &mut Out,
    config: &mut git2::Config,
    alias: &str,
) -> anyhow::Result<()> {
    writeln!(out, "Installing alias (non-global): git {}", alias)?;
    config
        .set_str(
            format!("alias.{}", alias).as_str(),
            format!("branchless {}", alias).as_str(),
        )
        .map_err(wrap_git_error)
        .with_context(|| format!("Setting alias {}", alias))?;
    Ok(())
}

#[context("Installing all aliases")]
fn install_aliases<Out: Write>(
    out: &mut Out,
    repo: &mut git2::Repository,
    git_executable: &GitExecutable,
) -> anyhow::Result<()> {
    let mut config = repo.config().with_context(|| "Getting repo config")?;
    install_alias(out, &mut config, "smartlog")?;
    install_alias(out, &mut config, "sl")?;
    install_alias(out, &mut config, "hide")?;
    install_alias(out, &mut config, "unhide")?;
    install_alias(out, &mut config, "prev")?;
    install_alias(out, &mut config, "next")?;
    install_alias(out, &mut config, "restack")?;
    install_alias(out, &mut config, "undo")?;

    let version_str = run_git_silent(repo, git_executable, &["version"])
        .with_context(|| "Determining Git version")?;
    let version: GitVersion = version_str
        .parse()
        .with_context(|| format!("Parsing Git version string: {}", version_str))?;
    if version < GitVersion(2, 29, 0) {
        write!(
            out,
            "\
{warning_str}: the branchless workflow's `git undo` command requires Git
v2.29 or later, but your Git version is: {version_str}

Some operations, such as branch updates, won't be correctly undone. Other
operations may be undoable. Attempt at your own risk.

Once you upgrade to Git v2.9, run `git branchless init` again. Any work you
do from then on will be correctly undoable.

This only applies to the `git undo` command. Other commands which are part of
the branchless workflow will work properly.
",
            warning_str = style("Warning").yellow().bold(),
            version_str = version_str,
        )?;
    }

    Ok(())
}

/// Initialize `git-branchless` in the current repo.
///
/// Args:
/// * `out`: The output stream to write to.
/// * `git_executable`: The path to the `git` executable on disk.
#[context("Initializing git-branchless for repo")]
fn init<Out: Write>(out: &mut Out, git_executable: &GitExecutable) -> anyhow::Result<()> {
    let mut repo = get_repo()?;
    install_hooks(out, &repo)?;
    install_aliases(out, &mut repo, git_executable)?;
    Ok(())
}

#[pyfunction]
fn py_init(py: Python, out: PyObject, git_executable: &str) -> PyResult<isize> {
    let mut text_io = TextIO::new(py, out);
    let git_executable = Path::new(git_executable);
    let git_executable = GitExecutable(git_executable.to_path_buf());
    let result = init(&mut text_io, &git_executable);
    let () = map_err_to_py_err(result, "Could not initialize git-branchless")?;
    Ok(0)
}

#[allow(missing_docs)]
pub fn register_python_symbols(module: &PyModule) -> PyResult<()> {
    module.add_function(pyo3::wrap_pyfunction!(py_init, module)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{update_between_lines, UPDATE_MARKER_END, UPDATE_MARKER_START};

    #[test]
    fn test_update_between_lines() {
        let input = format!(
            "\
hello, world
{}
contents 1
{}
goodbye, world
",
            UPDATE_MARKER_START, UPDATE_MARKER_END
        );
        let expected = format!(
            "\
hello, world
{}
contents 2
contents 3
{}
goodbye, world
",
            UPDATE_MARKER_START, UPDATE_MARKER_END
        );

        assert_eq!(
            update_between_lines(
                &input,
                "\
contents 2
contents 3
"
            ),
            expected
        )
    }
}
