//! Pure Rust GPT partition table manipulation.
//!
//! After `qemu-img resize` grows a raw disk image, the GPT partition table
//! still references the old disk size. This module rewrites the GPT so that
//! the largest partition fills all available space, which allows the existing
//! (already-expanded) EXT4 filesystem to be visible to the kernel.
//!
//! Filesystem resize is left to cloud-init (growpart + resize2fs) inside the VM.

use crate::error::{Error, Result};
use log::debug;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const SECTOR_SIZE: u64 = 512;
const GPT_HEADER_SIZE: usize = 92;
const GPT_PARTITION_ENTRY_SIZE: usize = 128;
const GPT_SIGNATURE: u64 = 0x5452415020494645; // "EFI PART"
const LINUX_FS_GUID: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
    0xE4,
];

/// CRC32 (ISO 3309 / ITU-T V.42) used by GPT.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn read_le_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap())
}

fn read_le_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buf[offset..offset + 8].try_into().unwrap())
}

fn write_le_u32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_le_u64(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

/// Grow the largest Linux filesystem partition to fill all available disk space.
///
/// This rewrites both the primary and backup GPT so the partition table matches
/// the actual disk size after a `qemu-img resize`.
pub fn grow_largest_partition(disk_path: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(disk_path)
        .map_err(|e| Error::Other(format!("Failed to open disk {}: {}", disk_path.display(), e)))?;

    let disk_size = file
        .seek(SeekFrom::End(0))
        .map_err(|e| Error::Other(format!("Failed to get disk size: {}", e)))?;
    let total_sectors = disk_size / SECTOR_SIZE;

    if total_sectors < 34 {
        return Err(Error::Other("Disk too small for GPT".to_string()));
    }

    // Read primary GPT header (LBA 1)
    let mut header = [0u8; SECTOR_SIZE as usize];
    file.seek(SeekFrom::Start(SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to GPT header: {}", e)))?;
    file.read_exact(&mut header)
        .map_err(|e| Error::Other(format!("Failed to read GPT header: {}", e)))?;

    let sig = read_le_u64(&header, 0);
    if sig != GPT_SIGNATURE {
        return Err(Error::Other(format!(
            "Not a valid GPT disk (signature: {:#x})",
            sig
        )));
    }

    let partition_entry_lba = read_le_u64(&header, 72);
    let num_entries = read_le_u32(&header, 80) as usize;
    let entry_size = read_le_u32(&header, 84) as usize;

    if entry_size < GPT_PARTITION_ENTRY_SIZE {
        return Err(Error::Other(format!(
            "Unexpected GPT entry size: {}",
            entry_size
        )));
    }

    // Read all partition entries
    let entries_bytes = num_entries * entry_size;
    let mut entries = vec![0u8; entries_bytes];
    file.seek(SeekFrom::Start(partition_entry_lba * SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to partition entries: {}", e)))?;
    file.read_exact(&mut entries)
        .map_err(|e| Error::Other(format!("Failed to read partition entries: {}", e)))?;

    // New last usable LBA: leave 33 sectors at the end for backup GPT
    // (1 header + 32 sectors for partition entries)
    let new_last_usable_lba = total_sectors - 34;

    // Find the largest Linux filesystem partition
    let mut best_idx: Option<usize> = None;
    let mut best_size: u64 = 0;

    for i in 0..num_entries {
        let off = i * entry_size;
        let type_guid = &entries[off..off + 16];

        // Skip empty entries
        if type_guid.iter().all(|&b| b == 0) {
            continue;
        }

        let first_lba = read_le_u64(&entries, off + 32);
        let last_lba = read_le_u64(&entries, off + 40);
        let size = last_lba.saturating_sub(first_lba);

        debug!(
            "Partition {}: type={:02x?}, first_lba={}, last_lba={}, size={}",
            i, &type_guid[..4], first_lba, last_lba, size
        );

        if type_guid == LINUX_FS_GUID && size > best_size {
            best_size = size;
            best_idx = Some(i);
        }
    }

    let idx = best_idx.ok_or_else(|| Error::Other("No Linux filesystem partition found".to_string()))?;
    let off = idx * entry_size;
    let old_last_lba = read_le_u64(&entries, off + 40);

    if old_last_lba >= new_last_usable_lba {
        debug!(
            "Partition {} already fills disk (last_lba={}, new_last_usable={})",
            idx, old_last_lba, new_last_usable_lba
        );
        return Ok(());
    }

    let first_lba = read_le_u64(&entries, off + 32);
    debug!(
        "Growing partition {}: LBA {}-{} -> {}-{}",
        idx, first_lba, old_last_lba, first_lba, new_last_usable_lba
    );

    // Update the partition's ending LBA
    write_le_u64(&mut entries, off + 40, new_last_usable_lba);

    // Compute new partition array CRC32
    let entries_crc = crc32(&entries);

    // Update primary GPT header
    // Offset 32: last usable LBA
    write_le_u64(&mut header, 32, new_last_usable_lba);
    // Offset 48: backup LBA (last sector of disk)
    write_le_u64(&mut header, 48, total_sectors - 1);
    // Offset 88: partition entries CRC32
    write_le_u32(&mut header, 88, entries_crc);
    // Zero out header CRC32 (offset 16) before computing it
    write_le_u32(&mut header, 16, 0);
    let header_crc = crc32(&header[..GPT_HEADER_SIZE]);
    write_le_u32(&mut header, 16, header_crc);

    // Write primary GPT header (LBA 1)
    file.seek(SeekFrom::Start(SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to write GPT header: {}", e)))?;
    file.write_all(&header)
        .map_err(|e| Error::Other(format!("Failed to write GPT header: {}", e)))?;

    // Write primary partition entries (LBA 2+)
    file.seek(SeekFrom::Start(partition_entry_lba * SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to write partition entries: {}", e)))?;
    file.write_all(&entries)
        .map_err(|e| Error::Other(format!("Failed to write partition entries: {}", e)))?;

    // Write backup partition entries (starts 32 sectors before last sector)
    let backup_entries_lba = total_sectors - 33;
    file.seek(SeekFrom::Start(backup_entries_lba * SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to write backup entries: {}", e)))?;
    file.write_all(&entries)
        .map_err(|e| Error::Other(format!("Failed to write backup partition entries: {}", e)))?;

    // Build backup GPT header (at last sector)
    let mut backup_header = header;
    // my_lba = last sector, alternate_lba = 1 (primary)
    write_le_u64(&mut backup_header, 24, total_sectors - 1); // my_lba
    write_le_u64(&mut backup_header, 40, 1); // alternate_lba
    write_le_u64(&mut backup_header, 48, 1); // alternate_lba for backup points to primary
    // partition_entry_lba for backup = backup_entries_lba
    write_le_u64(&mut backup_header, 72, backup_entries_lba);
    // Recompute header CRC
    write_le_u32(&mut backup_header, 16, 0);
    let backup_crc = crc32(&backup_header[..GPT_HEADER_SIZE]);
    write_le_u32(&mut backup_header, 16, backup_crc);

    // Write backup GPT header (last sector)
    file.seek(SeekFrom::Start((total_sectors - 1) * SECTOR_SIZE))
        .map_err(|e| Error::Other(format!("Failed to seek to write backup header: {}", e)))?;
    file.write_all(&backup_header)
        .map_err(|e| Error::Other(format!("Failed to write backup GPT header: {}", e)))?;

    // Update protective MBR partition 0 to cover the whole disk
    let mut mbr = [0u8; SECTOR_SIZE as usize];
    file.seek(SeekFrom::Start(0))
        .map_err(|e| Error::Other(format!("Failed to seek to MBR: {}", e)))?;
    file.read_exact(&mut mbr)
        .map_err(|e| Error::Other(format!("Failed to read MBR: {}", e)))?;

    // MBR partition entry 0 starts at offset 446, size field at offset 446+12
    let mbr_size = if total_sectors - 1 > 0xFFFFFFFF {
        0xFFFFFFFFu32
    } else {
        (total_sectors - 1) as u32
    };
    write_le_u32(&mut mbr, 446 + 12, mbr_size);

    file.seek(SeekFrom::Start(0))
        .map_err(|e| Error::Other(format!("Failed to seek to write MBR: {}", e)))?;
    file.write_all(&mbr)
        .map_err(|e| Error::Other(format!("Failed to write MBR: {}", e)))?;

    file.sync_all()
        .map_err(|e| Error::Other(format!("Failed to sync disk: {}", e)))?;

    debug!(
        "GPT partition {} grown to fill disk ({} sectors, {:.1} GiB)",
        idx,
        new_last_usable_lba - first_lba + 1,
        ((new_last_usable_lba - first_lba + 1) * SECTOR_SIZE) as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(crc32(&[]), 0x00000000);
    }

    #[test]
    fn test_crc32_known_value() {
        // CRC32 of "123456789" is 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_le_roundtrip_u32() {
        let mut buf = [0u8; 8];
        write_le_u32(&mut buf, 2, 0xDEADBEEF);
        assert_eq!(read_le_u32(&buf, 2), 0xDEADBEEF);
    }

    #[test]
    fn test_le_roundtrip_u64() {
        let mut buf = [0u8; 16];
        write_le_u64(&mut buf, 4, 0x0123456789ABCDEF);
        assert_eq!(read_le_u64(&buf, 4), 0x0123456789ABCDEF);
    }
}
