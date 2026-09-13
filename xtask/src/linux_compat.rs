//! GNU/Linux archive compatibility verification.

use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
    process::Command,
};

use flate2::read::GzDecoder;
use tar::Archive;

use super::release_targets::{Architecture, LibcFamily, ReleaseTarget};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct GlibcVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl std::fmt::Display for GlibcVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.patch == 0 {
            write!(formatter, "{}.{}", self.major, self.minor)
        } else {
            write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

pub(super) fn verify_archive(
    root: &Path,
    archive: &Path,
    target: ReleaseTarget,
) -> Result<(), String> {
    let temporary = verification_root()?;
    let binary = inspect_at(archive, temporary.path(), target)?;
    verify_version(root, &binary)
}

pub(super) fn inspect_archive(archive: &Path, target: ReleaseTarget) -> Result<String, String> {
    let temporary = verification_root()?;
    let binary = inspect_at(archive, temporary.path(), target)?;
    super::release::checksum(&binary)
}

fn verification_root() -> Result<tempfile::TempDir, String> {
    tempfile::Builder::new()
        .prefix("proqi-linux-compat-")
        .tempdir()
        .map_err(|error| format!("create Linux verification root: {error}"))
}

fn inspect_at(archive: &Path, output: &Path, target: ReleaseTarget) -> Result<PathBuf, String> {
    extract_archive(archive, output)?;
    let binary = extracted_binary(output, target);
    let metadata = binary
        .metadata()
        .map_err(|error| format!("inspect extracted {}: {error}", binary.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "Linux archive executable is not a regular file: {}",
            binary.display()
        ));
    }
    verify_elf_architecture(&binary, target.architecture)?;
    match target.libc {
        LibcFamily::Gnu => verify_symbol_ceiling(&binary)?,
        LibcFamily::Musl => verify_static_elf(&binary)?,
        LibcFamily::None => {
            return Err("Linux compatibility verifier requires a Linux libc target".to_owned());
        }
    }
    Ok(binary)
}

fn verify_elf_architecture(binary: &Path, architecture: Architecture) -> Result<(), String> {
    let output = Command::new("readelf")
        .arg("--file-header")
        .arg(binary)
        .output()
        .map_err(|error| format!("start readelf: {error}"))?;
    if !output.status.success() {
        return Err(format!("readelf exited with {}", output.status));
    }
    let header = String::from_utf8(output.stdout)
        .map_err(|error| format!("readelf header is not UTF-8: {error}"))?;
    if !elf_header_matches(&header, architecture) {
        return Err(format!(
            "ELF architecture does not match expected {}",
            match architecture {
                Architecture::X86_64 => "x86-64",
                Architecture::Arm64 => "ARM64",
            }
        ));
    }
    Ok(())
}

fn elf_header_matches(header: &str, architecture: Architecture) -> bool {
    let machine = match architecture {
        Architecture::X86_64 => "Advanced Micro Devices X86-64",
        Architecture::Arm64 => "AArch64",
    };
    header.contains("Class:                             ELF64")
        && header.contains(&format!("Machine:                           {machine}"))
}

fn extract_archive(archive: &Path, output: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| format!("open Linux archive: {error}"))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .map_err(|error| format!("read Linux archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read Linux archive member: {error}"))?;
        let unpacked = entry
            .unpack_in(output)
            .map_err(|error| format!("extract Linux archive member: {error}"))?;
        if !unpacked {
            return Err("Linux archive contains a path outside its package root".to_owned());
        }
    }
    Ok(())
}

fn extracted_binary(root: &Path, target: ReleaseTarget) -> PathBuf {
    root.join(format!("proqi-{}/proqi", target.triple))
}

fn verify_static_elf(binary: &Path) -> Result<(), String> {
    for arguments in [["--program-headers"], ["--dynamic"]] {
        let output = Command::new("readelf")
            .args(arguments)
            .arg(binary)
            .output()
            .map_err(|error| format!("start readelf: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "readelf exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("Requesting program interpreter") || text.contains("(NEEDED)") {
            return Err("musl fallback executable is dynamically linked".to_owned());
        }
    }
    println!("verified statically linked musl fallback");
    Ok(())
}

fn verify_symbol_ceiling(binary: &Path) -> Result<(), String> {
    let maximum = parse_version(super::release_targets::GLIBC_FLOOR)?;
    let output = Command::new("readelf")
        .args(["--version-info"])
        .arg(binary)
        .output()
        .map_err(|error| format!("start readelf: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "readelf exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("readelf output is not UTF-8: {error}"))?;
    let versions = required_versions(&stdout)?;
    let highest = versions
        .last()
        .copied()
        .ok_or_else(|| "ELF has no versioned glibc requirements".to_owned())?;
    if highest > maximum {
        return Err(format!(
            "ELF requires GLIBC_{highest}, exceeding supported ceiling GLIBC_{maximum}"
        ));
    }
    println!("verified glibc symbol ceiling: GLIBC_{highest} <= GLIBC_{maximum}");
    Ok(())
}

fn required_versions(output: &str) -> Result<BTreeSet<GlibcVersion>, String> {
    let mut versions = BTreeSet::new();
    for (index, _) in output.match_indices("GLIBC_") {
        let raw = output[index + "GLIBC_".len()..]
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '.')
            .collect::<String>();
        versions.insert(parse_version(&raw)?);
    }
    Ok(versions)
}

fn parse_version(raw: &str) -> Result<GlibcVersion, String> {
    let parts = raw
        .split('.')
        .map(|part| {
            part.parse::<u32>()
                .map_err(|error| format!("invalid glibc version `{raw}`: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [major, minor] => Ok(GlibcVersion {
            major: *major,
            minor: *minor,
            patch: 0,
        }),
        [major, minor, patch] => Ok(GlibcVersion {
            major: *major,
            minor: *minor,
            patch: *patch,
        }),
        _ => Err(format!("invalid glibc version `{raw}`")),
    }
}

fn verify_version(root: &Path, binary: &Path) -> Result<(), String> {
    let expected = format!("proqi {}", super::release::workspace_version(root)?);
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|error| format!("start extracted Linux binary: {error}"))?;
    let actual = String::from_utf8(output.stdout)
        .map_err(|error| format!("Linux version output is not UTF-8: {error}"))?;
    if output.status.success() && actual.trim() == expected {
        println!("verified extracted Linux archive: {expected}");
        Ok(())
    } else {
        Err(format!(
            "extracted Linux binary returned {} and `{}`; expected `{expected}`",
            output.status,
            actual.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_two_and_three_component_glibc_versions() {
        let maximum = parse_version(crate::release_targets::GLIBC_FLOOR).expect("floor");
        let versions =
            required_versions("Name: GLIBC_2.2.5 Flags: none\nName: GLIBC_2.35 Flags: none\n")
                .expect("versions");
        assert_eq!(
            versions.into_iter().collect::<Vec<_>>(),
            vec![
                GlibcVersion {
                    major: 2,
                    minor: 2,
                    patch: 5,
                },
                maximum,
            ]
        );
        assert!(parse_version("2.39").expect("version") > maximum);
        assert!(parse_version("2").is_err());
    }

    #[test]
    fn elf_header_architecture_must_match_exactly() {
        let x86 = "Class:                             ELF64\n\
                   Machine:                           Advanced Micro Devices X86-64\n";
        let arm = "Class:                             ELF64\n\
                   Machine:                           AArch64\n";
        assert!(elf_header_matches(x86, Architecture::X86_64));
        assert!(elf_header_matches(arm, Architecture::Arm64));
        assert!(!elf_header_matches(x86, Architecture::Arm64));
        assert!(!elf_header_matches(arm, Architecture::X86_64));
        assert!(!elf_header_matches(
            "Class:                             ELF32\nMachine:                           AArch64\n",
            Architecture::Arm64
        ));
    }
}
