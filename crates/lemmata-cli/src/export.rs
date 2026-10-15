//! `lemmata export --zip`: the project's source files in one zip, without
//! hidden files or build output.
//!
//! The arXiv bundle (`--arxiv`) isn't here yet: its logic lives in the desktop
//! app (apps/desktop/src-tauri/src/export.rs) and has to move into a crate first.

use anyhow::Context;
use lemmata_project::{Entry, Project};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

pub fn run(dir: &Path, zip: &Path) -> anyhow::Result<()> {
    let project = Project::open(dir).with_context(|| format!("opening {}", dir.display()))?;
    let mut names = Vec::new();
    collect(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &mut names,
    );
    write_stored(zip, &project.root, &names)?;
    println!("{} files written to {}", names.len(), zip.display());
    Ok(())
}

fn collect(entries: &[Entry], out: &mut Vec<String>) {
    for e in entries {
        if e.is_dir {
            collect(&e.children, out);
        } else {
            out.push(e.path.clone());
        }
    }
}

/// Writes `names` (relative to `root`) into a zip file. Entries are stored
/// without compression: the zip crate isn't a dependency of this CLI.
/// shortcut: no deflate, so the zip is larger than the app's export; add the
/// `zip` crate with its deflate feature once the CLI may take a new dependency.
fn write_stored(dest: &Path, root: &Path, names: &[String]) -> anyhow::Result<()> {
    const LOCAL: u32 = 0x0403_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    const END: u32 = 0x0605_4b50;
    // Flag bit 11: names are UTF-8. Date 1980-01-01 00:00, the zip epoch.
    const FLAGS: u16 = 0x0800;
    const DOS_DATE: u16 = 0x0021;

    let entries = u16::try_from(names.len()).context("too many files for one zip")?;
    let mut out = BufWriter::new(File::create(dest)?);
    let mut central = Vec::new();
    let mut offset: u64 = 0;

    for name in names {
        let data = fs::read(root.join(name)).with_context(|| format!("reading {name}"))?;
        let size = u32::try_from(data.len()).context("file too large for a zip without zip64")?;
        let crc = crc32(&data);
        let name_len = u16::try_from(name.len()).context("file name too long")?;
        let offset32 = u32::try_from(offset).context("zip larger than 4 GiB")?;

        // Local file header, then the data.
        let mut local = Vec::with_capacity(30 + name.len());
        push(&mut local, &LOCAL.to_le_bytes());
        push(&mut local, &[20, 0]); // version needed: 2.0
        push(&mut local, &FLAGS.to_le_bytes());
        push(&mut local, &[0, 0]); // method: stored
        push(&mut local, &[0, 0]); // time
        push(&mut local, &DOS_DATE.to_le_bytes());
        push(&mut local, &crc.to_le_bytes());
        push(&mut local, &size.to_le_bytes()); // compressed size
        push(&mut local, &size.to_le_bytes()); // uncompressed size
        push(&mut local, &name_len.to_le_bytes());
        push(&mut local, &[0, 0]); // extra field length
        push(&mut local, name.as_bytes());
        out.write_all(&local)?;
        out.write_all(&data)?;

        // The matching central directory record, written at the end.
        push(&mut central, &CENTRAL.to_le_bytes());
        push(&mut central, &[20, 0]); // version made by
        push(&mut central, &[20, 0]); // version needed
        push(&mut central, &FLAGS.to_le_bytes());
        push(&mut central, &[0, 0]); // method
        push(&mut central, &[0, 0]); // time
        push(&mut central, &DOS_DATE.to_le_bytes());
        push(&mut central, &crc.to_le_bytes());
        push(&mut central, &size.to_le_bytes());
        push(&mut central, &size.to_le_bytes());
        push(&mut central, &name_len.to_le_bytes());
        push(&mut central, &[0; 12]); // extra, comment, disk, attributes
        push(&mut central, &offset32.to_le_bytes());
        push(&mut central, name.as_bytes());

        offset += (local.len() + data.len()) as u64;
    }

    let central_offset = u32::try_from(offset).context("zip larger than 4 GiB")?;
    let central_size = u32::try_from(central.len()).context("zip larger than 4 GiB")?;
    out.write_all(&central)?;

    // End of central directory record.
    let mut end = Vec::with_capacity(22);
    push(&mut end, &END.to_le_bytes());
    push(&mut end, &[0; 4]); // disk numbers
    push(&mut end, &entries.to_le_bytes());
    push(&mut end, &entries.to_le_bytes());
    push(&mut end, &central_size.to_le_bytes());
    push(&mut end, &central_offset.to_le_bytes());
    push(&mut end, &[0, 0]); // comment length
    out.write_all(&end)?;
    out.flush()?;
    Ok(())
}

fn push(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

/// CRC-32 (IEEE, as zip uses it).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::crc32;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }
}
