use std::borrow::Cow;
use std::convert::TryInto;

use codepage_437::*;
use twoway::{find_bytes, rfind_bytes};

use crate::arch::usize;
use crate::result::*;

const EOCDR_MAGIC: [u8; 4] = [b'P', b'K', 5, 6];
const ZIP64_EOCDR_MAGIC: [u8; 4] = [b'P', b'K', 6, 6];
const ZIP64_EOCDR_LOCATOR_MAGIC: [u8; 4] = [b'P', b'K', 6, 7];
const CENTRAL_DIRECTORY_MAGIC: [u8; 4] = [b'P', b'K', 1, 2];

// Straight from the Rust docs:
fn read_u64(input: &mut &[u8]) -> u64 {
    let (int_bytes, rest) = input.split_at(std::mem::size_of::<u64>());
    *input = rest;
    u64::from_le_bytes(int_bytes.try_into().expect("less than eight bytes for u64"))
}

fn read_u32(input: &mut &[u8]) -> u32 {
    let (int_bytes, rest) = input.split_at(std::mem::size_of::<u32>());
    *input = rest;
    u32::from_le_bytes(int_bytes.try_into().expect("less than four bytes for u32"))
}

fn read_u16(input: &mut &[u8]) -> u16 {
    let (int_bytes, rest) = input.split_at(std::mem::size_of::<u16>());
    *input = rest;
    u16::from_le_bytes(int_bytes.try_into().expect("less than two bytes for u16"))
}

#[derive(Debug)]
pub struct EndOfCentralDirectory<'a> {
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub entries_on_this_disk: u16,
    pub entries: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub file_comment: &'a [u8],
}

impl<'a> EndOfCentralDirectory<'a> {
    pub fn parse(mut eocdr: &'a [u8]) -> ZipResult<Self> {
        // 4.3.16  End of central directory record:
        //
        // end of central dir signature    4 bytes  (0x06054b50)
        // number of this disk             2 bytes
        // number of the disk with the
        // start of the central directory  2 bytes
        // total number of entries in
        // the central dir on this disk    2 bytes
        // total number of entries in
        // the central dir                 2 bytes
        // size of the central directory   4 bytes
        // offset of start of central
        // directory with respect to
        // the starting disk number        4 bytes
        // zipfile comment length          2 bytes

        // Assert the magic instead of checking for it
        // because the search should have found it.
        assert_eq!(eocdr[..4], EOCDR_MAGIC);
        eocdr = &eocdr[4..];
        let disk_number = read_u16(&mut eocdr);
        let disk_with_central_directory = read_u16(&mut eocdr);
        let entries_on_this_disk = read_u16(&mut eocdr);
        let entries = read_u16(&mut eocdr);
        let central_directory_size = read_u32(&mut eocdr);
        let central_directory_offset = read_u32(&mut eocdr);
        let comment_length = read_u16(&mut eocdr);
        let file_comment = &eocdr[..usize(comment_length)?];

        Ok(Self {
            disk_number,
            disk_with_central_directory,
            entries_on_this_disk,
            entries,
            central_directory_size,
            central_directory_offset,
            file_comment,
        })
    }
}

pub fn find_eocdr(mapping: &[u8]) -> ZipResult<usize> {
    rfind_bytes(mapping, &EOCDR_MAGIC).ok_or(ZipError::InvalidArchive(
        "Couldn't find End Of Central Directory Record",
    ))
}

#[derive(Debug)]
pub struct Zip64EndOfCentralDirectoryLocator {
    pub disk_with_central_directory: u32,
    pub zip64_eocdr_offset: u64,
    pub disks: u32,
}

impl Zip64EndOfCentralDirectoryLocator {
    pub fn parse(mut mapping: &[u8]) -> Option<Self> {
        // 4.3.15 Zip64 end of central directory locator
        //
        // zip64 end of central dir locator
        // signature                       4 bytes  (0x07064b50)
        // number of the disk with the
        // start of the zip64 end of
        // central directory               4 bytes
        // relative offset of the zip64
        // end of central directory record 8 bytes
        // total number of disks           4 bytes
        if mapping[..4] != ZIP64_EOCDR_LOCATOR_MAGIC {
            return None;
        }
        mapping = &mapping[4..];
        let disk_with_central_directory = read_u32(&mut mapping);
        let zip64_eocdr_offset = read_u64(&mut mapping);
        let disks = read_u32(&mut mapping);

        Some(Self {
            disk_with_central_directory,
            zip64_eocdr_offset,
            disks,
        })
    }

    pub fn size_in_file() -> usize {
        20
    }
}

#[derive(Debug)]
pub struct Zip64EndOfCentralDirectory<'a> {
    pub source_version: u16,
    pub minimum_extract_version: u16,
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub entries_on_this_disk: u64,
    pub entries: u64,
    pub central_directory_size: u64,
    pub central_directory_offset: u64,
    pub extensible_data: &'a [u8],
}

impl<'a> Zip64EndOfCentralDirectory<'a> {
    pub fn parse(mut eocdr: &'a [u8]) -> ZipResult<Self> {
        // 4.3.14  Zip64 end of central directory record
        //
        // zip64 end of central dir
        // signature                       4 bytes  (0x06064b50)
        // size of zip64 end of central
        // directory record                8 bytes
        // version made by                 2 bytes
        // version needed to extract       2 bytes
        // number of this disk             4 bytes
        // number of the disk with the
        // start of the central directory  4 bytes
        // total number of entries in the
        // central directory on this disk  8 bytes
        // total number of entries in the
        // central directory               8 bytes
        // size of the central directory   8 bytes
        // offset of start of central
        // directory with respect to
        // the starting disk number        8 bytes
        // zip64 extensible data sector    (variable size)

        // Assert the magic instead of checking for it
        // because the search should have found it.
        assert_eq!(eocdr[..4], ZIP64_EOCDR_MAGIC);
        eocdr = &eocdr[4..];
        let eocdr_size = read_u64(&mut eocdr);
        let source_version = read_u16(&mut eocdr);
        let minimum_extract_version = read_u16(&mut eocdr);
        let disk_number = read_u32(&mut eocdr);
        let disk_with_central_directory = read_u32(&mut eocdr);
        let entries_on_this_disk = read_u64(&mut eocdr);
        let entries = read_u64(&mut eocdr);
        let central_directory_size = read_u64(&mut eocdr);
        let central_directory_offset = read_u64(&mut eocdr);

        // 4.3.14.1 The value stored into the "size of zip64 end of central
        // directory record" SHOULD be the size of the remaining
        // record and SHOULD NOT include the leading 12 bytes.
        //
        // Size = SizeOfFixedFields + SizeOfVariableData - 12.
        // (SizeOfVariableData = Size - SizeOfFixedFields + 12)

        // Check for underflow:
        let eocdr_size = usize(eocdr_size)?;
        if (eocdr_size + 12) < Self::fixed_size_in_file() {
            return Err(ZipError::InvalidArchive(
                "Invalid extensible data length in Zip64 End Of Central Directory Record",
            ));
        }
        // We should be left with just the extensible data:
        let extensible_data_length = eocdr_size + 12 - Self::fixed_size_in_file();
        if eocdr.len() != extensible_data_length {
            return Err(ZipError::InvalidArchive(
                "Invalid extensible data length in Zip64 End Of Central Directory Record",
            ));
        }
        let extensible_data = eocdr;

        Ok(Self {
            source_version,
            minimum_extract_version,
            disk_number,
            disk_with_central_directory,
            entries,
            entries_on_this_disk,
            central_directory_size,
            central_directory_offset,
            extensible_data,
        })
    }

    fn fixed_size_in_file() -> usize {
        56
    }
}

pub fn find_zip64_eocdr(mapping: &[u8]) -> ZipResult<usize> {
    find_bytes(mapping, &ZIP64_EOCDR_MAGIC).ok_or(ZipError::InvalidArchive(
        "Couldn't find zip64 End Of Central Directory Record",
    ))
}

pub struct CentralDirectoryEntry {}

impl CentralDirectoryEntry {
    pub fn parse_and_consume(entry: &mut &[u8]) -> ZipResult<Self> {
        // 4.3.12  Central directory structure:
        //
        // [central directory header 1]
        // .
        // .
        // .
        // [central directory header n]
        // [digital signature]
        //
        // File header:
        //
        //   central file header signature   4 bytes  (0x02014b50)
        //   version made by                 2 bytes
        //   version needed to extract       2 bytes
        //   general purpose bit flag        2 bytes
        //   compression method              2 bytes
        //   last mod file time              2 bytes
        //   last mod file date              2 bytes
        //   crc-32                          4 bytes
        //   compressed size                 4 bytes
        //   uncompressed size               4 bytes
        //   file name length                2 bytes
        //   extra field length              2 bytes
        //   file comment length             2 bytes
        //   disk number start               2 bytes
        //   internal file attributes        2 bytes
        //   external file attributes        4 bytes
        //   relative offset of local header 4 bytes
        //
        //   file name (variable size)
        //   extra field (variable size)
        //   file comment (variable size)
        if entry[..4] != CENTRAL_DIRECTORY_MAGIC {
            return Err(ZipError::InvalidArchive("Invalid central directory entry"));
        }
        *entry = &entry[4..];
        let source_version = read_u16(entry);
        let minimum_extract_version = read_u16(entry);
        let flags = read_u16(entry);
        let compression_method = read_u16(entry);
        let last_mod_time = read_u16(entry);
        let last_mod_date = read_u16(entry);
        let crc32 = read_u32(entry);
        let compressed_size = read_u32(entry);
        let uncompressed_size = read_u32(entry);
        let file_name_length = read_u16(entry) as usize;
        let extra_field_length = read_u16(entry) as usize;
        let file_comment_length = read_u16(entry) as usize;
        let disk_number = read_u16(entry);
        let internal_file_attributes = read_u16(entry);
        let external_file_attributes = read_u32(entry);
        let offset = read_u32(entry) as u64;
        let (file_name_raw, remaining) = entry.split_at(file_name_length);
        let (extra_field, remaining) = remaining.split_at(extra_field_length);
        let (file_comment_raw, remaining) = remaining.split_at(file_comment_length);
        *entry = remaining;

        // Done grabbing bytes. Let's decode some stuff:

        let encrypted = flags & 1 == 1;
        let is_utf8 = flags & (1 << 11) != 0;

        let file_name: Cow<str> = if is_utf8 {
            Cow::from(std::str::from_utf8(file_name_raw).map_err(|e| ZipError::Encoding(e))?)
        } else {
            Cow::borrow_from_cp437(file_name_raw, &CP437_CONTROL)
        };

        log::trace!("Entry for {:?}", file_name);

        Ok(Self {})
    }
}
