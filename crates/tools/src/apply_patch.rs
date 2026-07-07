use std::{
  fs,
  path::{Path, PathBuf},
};

use rmcp::model::{CallToolResult, ContentBlock, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tool_schema;

pub const APPLY_PATCH_TOOL_NAME: &str = "apply_patch";

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchArgs {
  /// Patch text using the Codex apply_patch format.
  pub patch: String,
  /// Optional base directory for relative patch paths.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub workdir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchOutput {
  pub applied: bool,
  pub changes: Vec<AppliedFileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AppliedFileChange {
  pub path: String,
  pub kind: AppliedFileChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppliedFileChangeKind {
  Add,
  Delete,
  Update,
  Move { to: String },
}

#[derive(Debug, Clone, Default)]
pub struct ApplyPatchTool;

impl ApplyPatchTool {
  pub fn tool(&self) -> Tool {
    apply_patch_tool()
  }

  pub fn call(&self, args: ApplyPatchArgs) -> CallToolResult {
    apply_patch(args)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatchOp {
  Add {
    path: PathBuf,
    lines: Vec<String>,
  },
  Delete {
    path: PathBuf,
  },
  Update {
    path: PathBuf,
    move_to: Option<PathBuf>,
    lines: Vec<PatchLine>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatchLine {
  Context(String),
  Add(String),
  Delete(String),
  HunkHeader,
  EndOfFile,
}

pub fn apply_patch_tool() -> Tool {
  Tool::new(
    APPLY_PATCH_TOOL_NAME,
    "Apply a local file patch using the Codex apply_patch format.",
    tool_schema::<ApplyPatchArgs>(),
  )
  .with_raw_output_schema(tool_schema::<ApplyPatchOutput>())
  .with_annotations(
    ToolAnnotations::new()
      .read_only(false)
      .destructive(true)
      .idempotent(false)
      .open_world(false),
  )
}

pub fn apply_patch(args: ApplyPatchArgs) -> CallToolResult {
  let workdir = args.workdir.unwrap_or_else(|| PathBuf::from("."));
  let ops = match parse_patch(&args.patch) {
    Ok(ops) => ops,
    Err(err) => return error_result(err),
  };

  let mut changes = Vec::new();
  for op in ops {
    let change = match apply_op(&workdir, op) {
      Ok(change) => change,
      Err(err) => return error_result(err),
    };
    changes.push(change);
  }

  let output = ApplyPatchOutput {
    applied: true,
    changes,
  };
  let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
    "Applied patch with {} file change(s).",
    output.changes.len()
  ))]);
  result.structured_content = Some(json!(output));
  result
}

fn parse_patch(patch: &str) -> Result<Vec<PatchOp>, String> {
  let lines = patch.lines().collect::<Vec<_>>();
  if lines.first() != Some(&"*** Begin Patch") {
    return Err("patch must start with `*** Begin Patch`".to_string());
  }
  if lines.last() != Some(&"*** End Patch") {
    return Err("patch must end with `*** End Patch`".to_string());
  }

  let mut ops = Vec::new();
  let mut i = 1;
  while i + 1 < lines.len() {
    let line = lines[i];
    if let Some(path) = line.strip_prefix("*** Add File: ") {
      i += 1;
      let mut add_lines = Vec::new();
      while i + 1 < lines.len() && !lines[i].starts_with("*** ") {
        let Some(content) = lines[i].strip_prefix('+') else {
          return Err(format!("add file line must start with `+`: {}", lines[i]));
        };
        add_lines.push(content.to_string());
        i += 1;
      }
      ops.push(PatchOp::Add {
        path: PathBuf::from(path),
        lines: add_lines,
      });
    } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
      ops.push(PatchOp::Delete {
        path: PathBuf::from(path),
      });
      i += 1;
    } else if let Some(path) = line.strip_prefix("*** Update File: ") {
      i += 1;
      let move_to = if i + 1 < lines.len() {
        lines[i].strip_prefix("*** Move to: ").map(PathBuf::from)
      } else {
        None
      };
      if move_to.is_some() {
        i += 1;
      }
      let mut patch_lines = Vec::new();
      while i + 1 < lines.len() && !lines[i].starts_with("*** ") {
        let current = lines[i];
        if current == "@@" || current.starts_with("@@ ") {
          patch_lines.push(PatchLine::HunkHeader);
        } else if current == "*** End of File" {
          patch_lines.push(PatchLine::EndOfFile);
        } else if let Some(content) = current.strip_prefix(' ') {
          patch_lines.push(PatchLine::Context(content.to_string()));
        } else if let Some(content) = current.strip_prefix('+') {
          patch_lines.push(PatchLine::Add(content.to_string()));
        } else if let Some(content) = current.strip_prefix('-') {
          patch_lines.push(PatchLine::Delete(content.to_string()));
        } else {
          return Err(format!("unsupported update line: {current}"));
        }
        i += 1;
      }
      ops.push(PatchOp::Update {
        path: PathBuf::from(path),
        move_to,
        lines: patch_lines,
      });
    } else {
      return Err(format!("unsupported patch header: {line}"));
    }
  }

  Ok(ops)
}

fn apply_op(workdir: &Path, op: PatchOp) -> Result<AppliedFileChange, String> {
  match op {
    PatchOp::Add { path, lines } => {
      let full_path = workdir.join(&path);
      if full_path.exists() {
        return Err(format!(
          "cannot add `{}` because it already exists",
          path.display()
        ));
      }
      if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
          format!(
            "failed to create parent directory for `{}`: {err}",
            path.display()
          )
        })?;
      }
      fs::write(&full_path, lines_to_text(lines))
        .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
      Ok(AppliedFileChange {
        path: path.display().to_string(),
        kind: AppliedFileChangeKind::Add,
      })
    }
    PatchOp::Delete { path } => {
      fs::remove_file(workdir.join(&path))
        .map_err(|err| format!("failed to delete `{}`: {err}", path.display()))?;
      Ok(AppliedFileChange {
        path: path.display().to_string(),
        kind: AppliedFileChangeKind::Delete,
      })
    }
    PatchOp::Update {
      path,
      move_to,
      lines,
    } => {
      let full_path = workdir.join(&path);
      let original = fs::read_to_string(&full_path)
        .map_err(|err| format!("failed to read `{}`: {err}", path.display()))?;
      let updated = apply_update_lines(&original, &lines)
        .map_err(|err| format!("failed to update `{}`: {err}", path.display()))?;
      fs::write(&full_path, updated)
        .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
      if let Some(move_to) = move_to {
        let full_move_to = workdir.join(&move_to);
        if let Some(parent) = full_move_to.parent() {
          fs::create_dir_all(parent).map_err(|err| {
            format!(
              "failed to create parent directory for `{}`: {err}",
              move_to.display()
            )
          })?;
        }
        fs::rename(&full_path, &full_move_to).map_err(|err| {
          format!(
            "failed to move `{}` to `{}`: {err}",
            path.display(),
            move_to.display()
          )
        })?;
        Ok(AppliedFileChange {
          path: path.display().to_string(),
          kind: AppliedFileChangeKind::Move {
            to: move_to.display().to_string(),
          },
        })
      } else {
        Ok(AppliedFileChange {
          path: path.display().to_string(),
          kind: AppliedFileChangeKind::Update,
        })
      }
    }
  }
}

fn apply_update_lines(original: &str, patch_lines: &[PatchLine]) -> Result<String, String> {
  let mut original_lines = split_text_lines(original);
  let trailing_newline = original.ends_with('\n');
  let mut cursor = 0;

  for chunk in patch_lines
    .split(|line| matches!(line, PatchLine::HunkHeader))
    .filter(|chunk| !chunk.is_empty())
  {
    let old_lines = chunk
      .iter()
      .filter_map(|line| match line {
        PatchLine::Context(text) | PatchLine::Delete(text) => Some(text.clone()),
        PatchLine::Add(_) | PatchLine::EndOfFile | PatchLine::HunkHeader => None,
      })
      .collect::<Vec<_>>();
    let new_lines = chunk
      .iter()
      .filter_map(|line| match line {
        PatchLine::Context(text) | PatchLine::Add(text) => Some(text.clone()),
        PatchLine::Delete(_) | PatchLine::EndOfFile | PatchLine::HunkHeader => None,
      })
      .collect::<Vec<_>>();

    let Some(position) = find_subsequence(&original_lines, &old_lines, cursor) else {
      return Err(format!("could not find patch context {:?}", old_lines));
    };
    original_lines.splice(position..position + old_lines.len(), new_lines.clone());
    cursor = position + new_lines.len();
  }

  let mut updated = original_lines.join("\n");
  if trailing_newline {
    updated.push('\n');
  }
  Ok(updated)
}

fn split_text_lines(text: &str) -> Vec<String> {
  text
    .strip_suffix('\n')
    .unwrap_or(text)
    .split('\n')
    .map(str::to_string)
    .collect()
}

fn find_subsequence(haystack: &[String], needle: &[String], start: usize) -> Option<usize> {
  if needle.is_empty() {
    return Some(start.min(haystack.len()));
  }
  haystack
    .windows(needle.len())
    .enumerate()
    .skip(start)
    .find_map(|(index, window)| (window == needle).then_some(index))
}

fn lines_to_text(lines: Vec<String>) -> String {
  if lines.is_empty() {
    String::new()
  } else {
    format!("{}\n", lines.join("\n"))
  }
}

fn error_result(message: impl Into<String>) -> CallToolResult {
  CallToolResult::error(vec![ContentBlock::text(message.into())])
}

#[cfg(test)]
mod tests {
  use std::{fs, path::PathBuf};

  use super::*;

  fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codexify-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
  }

  #[test]
  fn apply_patch_adds_and_updates_file() {
    let dir = temp_dir("apply-patch");
    let add = ApplyPatchTool.call(ApplyPatchArgs {
      patch: "*** Begin Patch\n*** Add File: a.txt\n+one\n+two\n*** End Patch".to_string(),
      workdir: Some(dir.clone()),
    });
    assert_eq!(add.is_error, Some(false));
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\ntwo\n");

    let update = ApplyPatchTool.call(ApplyPatchArgs {
      patch: "*** Begin Patch\n*** Update File: a.txt\n@@\n one\n-two\n+three\n*** End Patch"
        .to_string(),
      workdir: Some(dir.clone()),
    });
    assert_eq!(update.is_error, Some(false));
    assert_eq!(
      fs::read_to_string(dir.join("a.txt")).unwrap(),
      "one\nthree\n"
    );

    let _ = fs::remove_dir_all(dir);
  }
}
