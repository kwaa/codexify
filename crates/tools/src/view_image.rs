use std::{fs, path::PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::model::{CallToolResult, ContentBlock, Tool, ToolAnnotations};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tool_schema;

pub const VIEW_IMAGE_TOOL_NAME: &str = "view_image";

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ViewImageArgs {
  /// Local filesystem path to an image file.
  pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ViewImageTool;

impl ViewImageTool {
  pub fn tool(&self) -> Tool {
    view_image_tool()
  }

  pub fn call(&self, args: ViewImageArgs) -> CallToolResult {
    view_image(args)
  }
}

pub fn view_image_tool() -> Tool {
  Tool::new(
    VIEW_IMAGE_TOOL_NAME,
    "Read a local image file and return it as a data URL for visual inspection.",
    tool_schema::<ViewImageArgs>(),
  )
  .with_annotations(
    ToolAnnotations::new()
      .read_only(true)
      .destructive(false)
      .idempotent(true)
      .open_world(false),
  )
}

pub fn view_image(args: ViewImageArgs) -> CallToolResult {
  let bytes = match fs::read(&args.path) {
    Ok(bytes) => bytes,
    Err(err) => {
      return error_result(format!(
        "failed to read image `{}`: {err}",
        args.path.display()
      ));
    }
  };
  let mime_type = match mime_type_for_path(&args.path) {
    Some(mime_type) => mime_type,
    None => {
      return error_result(format!(
        "unsupported image extension for `{}`",
        args.path.display()
      ));
    }
  };
  let encoded = BASE64_STANDARD.encode(bytes);
  CallToolResult::success(vec![ContentBlock::image(encoded, mime_type)])
}

fn mime_type_for_path(path: &PathBuf) -> Option<&'static str> {
  match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
    "jpg" | "jpeg" => Some("image/jpeg"),
    "png" => Some("image/png"),
    "gif" => Some("image/gif"),
    "webp" => Some("image/webp"),
    "bmp" => Some("image/bmp"),
    _ => None,
  }
}

fn error_result(message: impl Into<String>) -> CallToolResult {
  CallToolResult::error(vec![ContentBlock::text(message.into())])
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  #[test]
  fn view_image_returns_mcp_image_content() {
    let path = std::env::temp_dir().join(format!("codexify-view-image-{}.png", std::process::id()));
    fs::write(&path, [137, 80, 78, 71]).expect("write image bytes");

    let result = ViewImageTool.call(ViewImageArgs { path: path.clone() });

    let _ = fs::remove_file(path);
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);
    let ContentBlock::Image(image) = &result.content[0] else {
      panic!("expected image content block");
    };
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.data, "iVBORw==");
    assert!(result.structured_content.is_none());
  }
}
