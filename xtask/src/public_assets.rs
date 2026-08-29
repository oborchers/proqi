//! Deterministic checks for public README and repository presentation assets.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const GIF_LIMIT: u64 = 12 * 1024 * 1024;
const PREVIEW_LIMIT: u64 = 2 * 1024 * 1024;
const REQUIRED_ASSETS: &[&str] = &[
    "assets/proqi-demo.gif",
    "assets/proqi-demo-storyboard.md",
    "assets/codex-composer.png",
    "assets/proqi-logo.png",
    "assets/proqi-problem.svg",
    "assets/proqi-social-preview.png",
    "assets/proqi-social-preview.svg",
    "scripts/readme-demo-seed.exp",
];
const PUBLIC_TEXT: &[&str] = &[
    "README.md",
    "assets/proqi-demo-storyboard.md",
    "assets/proqi-problem.svg",
    "assets/proqi-social-preview.svg",
    "scripts/readme-demo-record.exp",
    "scripts/readme-demo-seed.exp",
    "scripts/readme-demo.sh",
    "scripts/social-preview.sh",
];
const FORBIDDEN: &[&str] = &[
    "/Users/",
    "Code.nosync",
    "NSIRD_",
    "TemporaryItems",
    "private alpha",
];

pub(crate) fn check(root: &Path) -> Result<(), String> {
    require_assets(root)?;
    check_public_text(root)?;
    check_readme_links(root)?;
    check_problem_flow(root)?;
    check_demo_contract(root)?;
    check_dimensions_and_sizes(root)?;
    check_shell(root, "scripts/readme-demo.sh")?;
    check_shell(root, "scripts/social-preview.sh")?;
    println!("public assets: links, direction, privacy, dimensions, and scripts are valid");
    Ok(())
}

fn require_assets(root: &Path) -> Result<(), String> {
    for relative in REQUIRED_ASSETS {
        let path = root.join(relative);
        if !path.is_file() {
            return Err(format!("required public asset is missing: {relative}"));
        }
    }
    Ok(())
}

fn check_public_text(root: &Path) -> Result<(), String> {
    for relative in PUBLIC_TEXT {
        let text = read_text(root, relative)?;
        for forbidden in FORBIDDEN {
            if text.contains(forbidden) {
                return Err(format!(
                    "{relative} contains forbidden public text `{forbidden}`"
                ));
            }
        }
    }
    Ok(())
}

fn check_readme_links(root: &Path) -> Result<(), String> {
    let readme = read_text(root, "README.md")?;
    for linked in linked_assets(&readme) {
        if !root.join(&linked).is_file() {
            return Err(format!("README links missing asset: {}", linked.display()));
        }
    }
    for required in [
        "assets/proqi-logo.png",
        "assets/proqi-demo.gif",
        "assets/codex-composer.png",
    ] {
        if !readme.contains(required) {
            return Err(format!("README does not link required asset: {required}"));
        }
    }
    Ok(())
}

fn linked_assets(readme: &str) -> Vec<PathBuf> {
    let mut paths = extract_after(readme, "src=\"assets/", '"');
    paths.extend(extract_after(readme, "](assets/", ')'));
    paths
        .into_iter()
        .map(|suffix| PathBuf::from(format!("assets/{suffix}")))
        .collect()
}

fn extract_after(text: &str, marker: &str, end: char) -> Vec<String> {
    let mut remaining = text;
    let mut values = Vec::new();
    while let Some(start) = remaining.find(marker) {
        let value = &remaining[start + marker.len()..];
        let Some(length) = value.find(end) else {
            break;
        };
        values.push(value[..length].to_owned());
        remaining = &value[length + end.len_utf8()..];
    }
    values
}

fn check_problem_flow(root: &Path) -> Result<(), String> {
    let svg = read_text(root, "assets/proqi-problem.svg")?;
    for marker in [
        "data-flow=\"proqi-to-agent\"",
        "Copy or submit from Proqi to the coding agent",
        "coding-agent terminal",
        "received follow-up",
    ] {
        if !svg.contains(marker) {
            return Err(format!("problem diagram lost direction marker: {marker}"));
        }
    }
    if svg.contains("agent-to-proqi") {
        return Err("problem diagram reverses the Proqi-to-agent flow".to_owned());
    }
    Ok(())
}

fn check_demo_contract(root: &Path) -> Result<(), String> {
    let wrapper = read_text(root, "scripts/readme-demo.sh")?;
    let recorder = read_text(root, "scripts/readme-demo-record.exp")?;
    let seed = read_text(root, "scripts/readme-demo-seed.exp")?;
    for marker in [
        "asciinema record",
        "agg --quiet --theme github-dark",
        "unset NO_COLOR",
        "--window-size 92x30",
        "fc-match",
        "Meslo LG M DZ for Powerline",
    ] {
        if !wrapper.contains(marker) {
            return Err(format!("README demo lost required scene: {marker}"));
        }
    }
    for marker in [
        "foreground #c9cde0",
        "background #181922",
        "send -- \"y\"",
        "send -- \"\\033\\[100;9u\"",
    ] {
        if !recorder.contains(marker) {
            return Err(format!("README demo lost recorder contract: {marker}"));
        }
    }
    for marker in ["demo-image.png", "Context line 13.", "\\033\\[200~"] {
        if !seed.contains(marker) {
            return Err(format!("README demo lost seed contract: {marker}"));
        }
    }
    Ok(())
}

fn check_dimensions_and_sizes(root: &Path) -> Result<(), String> {
    let composer = root.join("assets/codex-composer.png");
    require_dimensions(&composer, image_dimensions(&composer)?, (752, 353))?;

    let gif = root.join("assets/proqi-demo.gif");
    require_dimensions(&gif, image_dimensions(&gif)?, (1132, 775))?;
    require_max_size(&gif, GIF_LIMIT)?;

    let preview = root.join("assets/proqi-social-preview.png");
    require_dimensions(&preview, image_dimensions(&preview)?, (1280, 640))?;
    require_max_size(&preview, PREVIEW_LIMIT)
}

fn image_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    dimensions_from_bytes(&bytes)
        .ok_or_else(|| format!("unsupported or truncated image: {}", path.display()))
}

fn dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes
        .get(..6)
        .is_some_and(|magic| magic == b"GIF87a" || magic == b"GIF89a")
    {
        let width = u16::from_le_bytes([*bytes.get(6)?, *bytes.get(7)?]);
        let height = u16::from_le_bytes([*bytes.get(8)?, *bytes.get(9)?]);
        return Some((u32::from(width), u32::from(height)));
    }
    if bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n") {
        let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        return Some((width, height));
    }
    None
}

fn require_dimensions(path: &Path, actual: (u32, u32), expected: (u32, u32)) -> Result<(), String> {
    (actual == expected).then_some(()).ok_or_else(|| {
        format!(
            "{} is {}x{}, expected {}x{}",
            path.display(),
            actual.0,
            actual.1,
            expected.0,
            expected.1
        )
    })
}

fn require_max_size(path: &Path, limit: u64) -> Result<(), String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?
        .len();
    (size <= limit)
        .then_some(())
        .ok_or_else(|| format!("{} is {size} bytes, limit is {limit}", path.display()))
}

fn check_shell(root: &Path, relative: &str) -> Result<(), String> {
    let status = Command::new("sh")
        .args(["-n", relative])
        .current_dir(root)
        .status()
        .map_err(|error| format!("start sh for {relative}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("shell syntax check failed: {relative}"))
}

fn read_text(root: &Path, relative: &str) -> Result<String, String> {
    fs::read_to_string(root.join(relative)).map_err(|error| format!("read {relative}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gif_and_png_dimensions_without_image_dependencies() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend([176, 4, 32, 3]);
        assert_eq!(dimensions_from_bytes(&gif), Some((1200, 800)));

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend([0; 8]);
        png.extend(1280_u32.to_be_bytes());
        png.extend(640_u32.to_be_bytes());
        assert_eq!(dimensions_from_bytes(&png), Some((1280, 640)));
    }

    #[test]
    fn extracts_html_and_markdown_asset_links() {
        let links = linked_assets("<img src=\"assets/a.png\"> [b](assets/b.svg)");
        assert_eq!(
            links,
            vec![PathBuf::from("assets/a.png"), PathBuf::from("assets/b.svg")]
        );
    }
}
