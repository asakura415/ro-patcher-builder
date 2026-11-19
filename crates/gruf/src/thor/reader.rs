use std::boxed::Box;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::thor::{
    ThorMode, INTEGRITY_FILE_NAME, MULTIPLE_FILES_TABLE_DESC_SIZE, THOR_HEADER_MAGIC,
};
use crate::{GrufError, Result};
use crc::crc32;
use encoding::label::encoding_from_whatwg_label;
use encoding::DecoderTrap;
use flate2::read::ZlibDecoder;
use nom::error::ErrorKind;
use nom::number::complete::{le_i16, le_i32, le_u32, le_u8};
use nom::*;

// Packed structs' sizes in bytes
const MAX_FILE_NAME_SIZE: usize = 256;
const HEADER_MAX_SIZE: usize = THOR_HEADER_MAGIC.len() + 0x8 + MAX_FILE_NAME_SIZE;
const SINGLE_FILE_ENTRY_MAX_SIZE: usize = 9 + MAX_FILE_NAME_SIZE;

// Defensive bounds for untrusted archive headers/tables/entries
const MAX_THOR_TABLE_COMPRESSED_SIZE: usize = 64 * 1024 * 1024;
const MAX_THOR_TABLE_DECOMPRESSED_SIZE: usize = 256 * 1024 * 1024;
const MAX_THOR_ENTRY_SIZE: usize = 512 * 1024 * 1024;

pub type ThorPatchList = Vec<ThorPatchInfo>;

/// Parses Thor's plist.txt file
pub fn patch_list_from_string(content: &str) -> ThorPatchList {
    let vec_lines: Vec<&str> = content.lines().collect();
    let mut sorted_patch_list: ThorPatchList = vec_lines
        .into_iter()
        .filter_map(|elem| ThorPatchInfo::from_string(&elem))
        .collect();
    // Sort patch list by index
    sorted_patch_list.sort_by(|a, b| a.index.cmp(&b.index));
    sorted_patch_list
}

#[derive(Debug)]
pub struct ThorPatchInfo {
    pub index: usize,
    pub file_name: String,
}

impl ThorPatchInfo {
    /// Parses a line to extract patch index and patch file name.
    /// Returns a PatchInfo struct in case of success.
    /// Returns None in case of failure
    fn from_string(line: &str) -> Option<ThorPatchInfo> {
        let words: Vec<_> = line.trim().split_whitespace().collect();
        let index_str = words.get(0)?;
        let index = match str::parse(index_str) {
            Ok(v) => v,
            Err(_) => {
                return None;
            }
        };
        let file_name = words.get(1)?;
        Some(ThorPatchInfo {
            index,
            file_name: (*file_name).to_string(),
        })
    }
}

fn parse_data_integrity_info(data: &str) -> HashMap<&str, u32> {
    data.lines()
        .filter_map(|line| {
            let (file_name, hash_str) = line.trim().rsplit_once('=')?;
            let hash = u32::from_str_radix(hash_str.trim().trim_start_matches("0x"), 16).ok()?;
            Some((file_name, hash))
        })
        .collect()
}

#[derive(Debug)]
pub struct ThorArchive<R: ?Sized> {
    obj: Box<R>,
    container: ThorContainer,
}

impl ThorArchive<File> {
    pub fn open(thor_archive_path: &Path) -> Result<ThorArchive<File>> {
        let file = File::open(thor_archive_path)?;
        ThorArchive::new(file)
    }
}

impl<R: Read + Seek> ThorArchive<R> {
    /// Create a new archive with the underlying object as the reader.
    pub fn new(mut obj: R) -> Result<ThorArchive<R>> {
        let thor_patch = parse_thor_patch(&mut obj)?;
        Ok(ThorArchive {
            obj: Box::new(obj),
            container: thor_patch,
        })
    }

    pub fn use_grf_merging(&self) -> bool {
        self.container.header.use_grf_merging
    }

    pub fn file_count(&self) -> usize {
        self.container.entries.len()
    }

    pub fn target_grf_name(&self) -> String {
        self.container.header.target_grf_name.clone()
    }

    pub fn get_entry_raw_data<S: AsRef<str> + Hash>(&mut self, file_path: S) -> Result<Vec<u8>> {
        let file_entry = self
            .get_file_entry(file_path)
            .ok_or(GrufError::EntryNotFound)?
            .clone();
        if file_entry.size_compressed == 0 {
            return Ok(vec![]);
        }
        if file_entry.size_compressed > MAX_THOR_ENTRY_SIZE || file_entry.size > MAX_THOR_ENTRY_SIZE
        {
            return Err(GrufError::parsing_error(
                "THOR entry size exceeds maximum allowed limit",
            ));
        }

        let stream_len = self.obj.seek(SeekFrom::End(0))?;
        let entry_end = file_entry
            .offset
            .checked_add(file_entry.size_compressed as u64)
            .ok_or_else(|| GrufError::parsing_error("THOR entry offset overflow"))?;
        if entry_end > stream_len {
            return Err(GrufError::parsing_error(
                "THOR entry extends past end of archive",
            ));
        }

        self.obj.seek(SeekFrom::Start(file_entry.offset))?;
        let mut content = vec![0u8; file_entry.size_compressed];
        self.obj.read_exact(&mut content)?;
        Ok(content)
    }

    pub fn read_file_content<S: AsRef<str> + Hash>(&mut self, file_path: S) -> Result<Vec<u8>> {
        let file_entry = self
            .get_file_entry(file_path)
            .ok_or(GrufError::EntryNotFound)?
            .clone();
        if file_entry.size_compressed == 0 {
            if file_entry.size != 0 {
                return Err(GrufError::parsing_error(
                    "THOR entry has zero compressed size for non-empty payload",
                ));
            }
            return Ok(vec![]);
        }
        if file_entry.size_compressed > MAX_THOR_ENTRY_SIZE || file_entry.size > MAX_THOR_ENTRY_SIZE
        {
            return Err(GrufError::parsing_error(
                "THOR entry size exceeds maximum allowed limit",
            ));
        }

        let stream_len = self.obj.seek(SeekFrom::End(0))?;
        let entry_end = file_entry
            .offset
            .checked_add(file_entry.size_compressed as u64)
            .ok_or_else(|| GrufError::parsing_error("THOR entry offset overflow"))?;
        if entry_end > stream_len {
            return Err(GrufError::parsing_error(
                "THOR entry extends past end of archive",
            ));
        }

        self.obj.seek(SeekFrom::Start(file_entry.offset))?;
        let mut content = vec![0u8; file_entry.size_compressed];
        self.obj.read_exact(&mut content)?;
        // Decompress the content with zlib (bounded to declared size + 1 to detect bombs)
        let decoder = ZlibDecoder::new(content.as_slice());
        let mut decompressed_content = Vec::with_capacity(file_entry.size.min(16 * 1024 * 1024));
        let decompressed_size = decoder
            .take(file_entry.size as u64 + 1)
            .read_to_end(&mut decompressed_content)?;
        if decompressed_size != file_entry.size {
            return Err(GrufError::parsing_error(
                "Decompressed content is not as expected",
            ));
        }
        Ok(decompressed_content)
    }

    pub fn extract_file<S: AsRef<str> + Hash>(
        &mut self,
        file_path: S,
        destination_path: &Path,
    ) -> Result<()> {
        let content = self.read_file_content(file_path)?;
        let mut file = File::create(destination_path)?;
        Ok(file.write_all(content.as_slice())?)
    }

    pub fn get_file_entry<S: AsRef<str> + Hash>(&self, file_path: S) -> Option<&ThorFileEntry> {
        self.container.entries.get(file_path.as_ref())
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &'_ ThorFileEntry> {
        self.container.entries.values()
    }

    /// Checks if the container has been unintentionnaly corrupted
    pub fn is_valid(&mut self) -> Result<bool> {
        let required_entries: Vec<String> = self
            .container
            .entries
            .values()
            .filter(|e| !e.is_internal() && !e.is_removed)
            .map(|e| e.relative_path.clone())
            .collect();

        let integrity_data = self.read_file_content(INTEGRITY_FILE_NAME)?;
        let integrity_data_as_str = string_from_win_1252(integrity_data.as_slice())?;
        let integrity_info = parse_data_integrity_info(integrity_data_as_str.as_str());

        for rel_path in &required_entries {
            if !integrity_info.contains_key(rel_path.as_str()) {
                return Ok(false);
            }
        }

        for (file_path, hash) in integrity_info {
            let file_content = match self.read_file_content(file_path) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            };
            if crc32::checksum_ieee(file_content.as_slice()) != hash {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ThorContainer {
    pub header: ThorHeader,
    table: ThorTable,
    pub entries: HashMap<String, ThorFileEntry>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ThorHeader {
    pub use_grf_merging: bool, // false -> client directory, true -> GRF
    pub file_count: usize,
    mode: ThorMode,
    pub target_grf_name: String, // If empty (size == 0) -> default GRF
}

#[derive(Debug, PartialEq, Eq)]
enum ThorTable {
    SingleFile(SingleFileTableDesc),
    MultipleFiles(MultipleFilesTableDesc),
}

#[derive(Debug, PartialEq, Eq)]
struct SingleFileTableDesc {
    file_table_offset: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct MultipleFilesTableDesc {
    file_table_compressed_size: usize,
    file_table_offset: u64,
}

#[derive(Debug, Clone, Eq)]
pub struct ThorFileEntry {
    pub size_compressed: usize,
    pub size: usize,
    pub relative_path: String,
    pub is_removed: bool,
    pub offset: u64,
}

impl ThorFileEntry {
    pub fn is_internal(&self) -> bool {
        self.relative_path == INTEGRITY_FILE_NAME
    }
}

impl Hash for ThorFileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.relative_path.hash(state);
    }
}

impl PartialEq for ThorFileEntry {
    fn eq(&self, other: &ThorFileEntry) -> bool {
        self.relative_path == other.relative_path
    }
}

fn i16_to_thor_mode(i: i16) -> ThorMode {
    match i {
        33 => ThorMode::SingleFile,
        48 => ThorMode::MultipleFiles,
        _ => ThorMode::Invalid,
    }
}

/// Checks entries' flags
/// If LSB is 1, the entry indicates a file deletion
fn is_file_removed(flags: u8) -> bool {
    (flags & 0b1) == 1
}

named!(parse_thor_header<&[u8], ThorHeader>,
    do_parse!(
        tag!(THOR_HEADER_MAGIC)
            >> use_grf_merging: le_u8
            >> file_count: le_u32
            >> mode: le_i16
            >> target_grf_name_size: le_u8
            >> target_grf_name: take_str!(target_grf_name_size)
            >> (ThorHeader {
                use_grf_merging: use_grf_merging == 1,
                file_count: file_count as usize,
                mode: i16_to_thor_mode(mode),
                target_grf_name: target_grf_name.to_string(),
            }
    )
));

named!(parse_single_file_table<&[u8], SingleFileTableDesc>,
    do_parse!(
        take!(1)
        >> (SingleFileTableDesc {
            file_table_offset: 0, // Offset in the 'data' field
        }
    )
));

fn parse_non_negative_usize(input: &[u8]) -> IResult<&[u8], usize> {
    let (rest, val) = le_i32(input)?;
    let non_neg =
        usize::try_from(val).map_err(|_| nom::Err::Failure((input, ErrorKind::Verify)))?;
    Ok((rest, non_neg))
}

fn parse_non_negative_u64(input: &[u8]) -> IResult<&[u8], u64> {
    let (rest, val) = le_i32(input)?;
    let non_neg =
        u64::try_from(val).map_err(|_| nom::Err::Failure((input, ErrorKind::Verify)))?;
    Ok((rest, non_neg))
}

named!(parse_multiple_files_table<&[u8], MultipleFilesTableDesc>,
    do_parse!(
        file_table_compressed_size: parse_non_negative_usize
        >> file_table_offset: parse_non_negative_u64
        >> (MultipleFilesTableDesc {
            file_table_compressed_size,
            file_table_offset, // Offset in the 'data' field
        }
    )
));

fn string_from_win_1252(v: &[u8]) -> Result<String> {
    if let Some(decoder) = encoding_from_whatwg_label("windows-1252") {
        if let Ok(s) = decoder.decode(v, DecoderTrap::Strict) {
            return Ok(s);
        }
    }
    // Fallback to lossless 1-to-1 byte-to-char mapping if CP949 bytes include 0x81/0x8D/0x8F/0x90/0x9D
    Ok(v.iter().map(|&b| char::from(b)).collect())
}

macro_rules! take_string_ansi (
    ( $i:expr, $size:expr ) => (
        {
            let input: &[u8] = $i;
            map_res!(input, take!($size), string_from_win_1252)
        }
     );
);

named!(parse_single_file_entry<&[u8], ThorFileEntry>,
    do_parse!(
        size_compressed: parse_non_negative_usize
        >> size: parse_non_negative_usize
        >> relative_path_size: le_u8
        >> relative_path: take_string_ansi!(relative_path_size)
        >> (ThorFileEntry {
            size_compressed,
            size,
            relative_path,
            is_removed: false,
            offset: 0, // This field is set outside the parser
        }
    )
));

/// Uses the given parser only if the flag is as expected
/// This is used to avoid parsing unexisting fields for files marked for deletion
macro_rules! take_if_not_removed (
    ( $i:expr, $parser:expr, $flags:expr ) => (
        {
            let input: &[u8] = $i;
            if is_file_removed($flags) {
                value!(input, 0)
            } else {
                $parser(input)
            }
        }
        );
);

named!(parse_multiple_files_entry<&[u8], ThorFileEntry>,
    do_parse!(
        relative_path_size: le_u8
        >> relative_path: take_string_ansi!(relative_path_size)
        >> flags: le_u8
        >> offset: take_if_not_removed!(le_u32, flags)
        >> size_compressed: take_if_not_removed!(parse_non_negative_usize, flags)
        >> size: take_if_not_removed!(parse_non_negative_usize, flags)
        >> (ThorFileEntry {
            size_compressed,
            size,
            relative_path,
            is_removed: is_file_removed(flags),
            offset: offset as u64,
        }
    )
));

named!(parse_multiple_files_entries<&[u8], HashMap<String, ThorFileEntry>>,
    fold_many1!(parse_multiple_files_entry, HashMap::new(), |mut acc: HashMap<_, _>, item| {
        acc.insert(item.relative_path.clone(), item);
        acc
    })
);

pub fn parse_thor_patch<R: Seek + Read>(reader: &mut R) -> Result<ThorContainer> {
    const HEADER_EXTENDED_MAX_SIZE: usize =
        HEADER_MAX_SIZE + MULTIPLE_FILES_TABLE_DESC_SIZE + SINGLE_FILE_ENTRY_MAX_SIZE;
    let stream_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;

    let mut thor_header_buf = Vec::with_capacity(HEADER_EXTENDED_MAX_SIZE);
    let mut reader_chunk = reader.take(thor_header_buf.capacity() as u64);
    reader_chunk.read_to_end(&mut thor_header_buf)?;
    let (output, header) = parse_thor_header(&thor_header_buf)
        .map_err(|_| GrufError::parsing_error("Failed to parse THOR header"))?;
    match header.mode {
        ThorMode::Invalid => Err(GrufError::parsing_error("Invalid THOR header mode")),
        ThorMode::SingleFile => {
            // Parse table
            let (output, table) = parse_single_file_table(output)
                .map_err(|_| GrufError::parsing_error("Failed to parse THOR file table"))?;
            // Parse the single entry
            let (output, mut entry) = parse_single_file_entry(output)
                .map_err(|_| GrufError::parsing_error("Failed to parse THOR file entry"))?;
            entry.offset = output.as_ptr() as u64 - thor_header_buf.as_ptr() as u64;
            Ok(ThorContainer {
                header,
                table: ThorTable::SingleFile(table),
                entries: [(entry.relative_path.clone(), entry)]
                    .iter()
                    .cloned()
                    .collect(),
            })
        }
        ThorMode::MultipleFiles => {
            let (output, table) = parse_multiple_files_table(output)
                .map_err(|_| GrufError::parsing_error("Failed to parse THOR file table"))?;
            let consumed_bytes = output.as_ptr() as u64 - thor_header_buf.as_ptr() as u64;
            if table.file_table_offset < consumed_bytes || table.file_table_offset > stream_len {
                return Err(GrufError::parsing_error("Invalid THOR file table offset"));
            }
            if table.file_table_compressed_size > MAX_THOR_TABLE_COMPRESSED_SIZE {
                return Err(GrufError::parsing_error(
                    "THOR file table compressed size exceeds maximum allowed size",
                ));
            }
            let table_end = table
                .file_table_offset
                .checked_add(table.file_table_compressed_size as u64)
                .ok_or_else(|| GrufError::parsing_error("THOR file table offset overflow"))?;
            if table_end > stream_len {
                return Err(GrufError::parsing_error(
                    "THOR file table extends past end of archive",
                ));
            }
            // Decompress the table with zlib (bounded)
            reader.seek(SeekFrom::Start(table.file_table_offset))?;
            let mut compressed_table = vec![0u8; table.file_table_compressed_size];
            reader.read_exact(&mut compressed_table)?;
            let decoder = ZlibDecoder::new(compressed_table.as_slice());
            let mut decompressed_table = Vec::new();
            let decompressed_size = decoder
                .take(MAX_THOR_TABLE_DECOMPRESSED_SIZE as u64 + 1)
                .read_to_end(&mut decompressed_table)?;
            if decompressed_size > MAX_THOR_TABLE_DECOMPRESSED_SIZE {
                return Err(GrufError::parsing_error(
                    "THOR file table exceeds maximum allowed size",
                ));
            }
            // Parse multiple entries
            let entries = match decompressed_size {
                0 => HashMap::new(), // No entries
                _ => {
                    let (_, entries) = parse_multiple_files_entries(decompressed_table.as_slice())
                        .map_err(|_| {
                            GrufError::parsing_error("Failed to parse THOR file entries")
                        })?;
                    entries
                }
            };
            Ok(ThorContainer {
                header,
                table: ThorTable::MultipleFiles(table),
                entries,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_patch_list_from_string() {
        let plist_content = "//869 iteminfo_20170423.thor
870 iteminfo_20170423_.thor
871 sprites_20170427.thor
872 sprites_20170429.thor
623 2016-01_01.thor
//873 rodex_20170501.thor
//874 strings_20170501.thor
875 rodex_20170501_.thor";
        let expected_content: HashMap<usize, &str> = [
            (623, "2016-01_01.thor"),
            (870, "iteminfo_20170423_.thor"),
            (871, "sprites_20170427.thor"),
            (872, "sprites_20170429.thor"),
            (875, "rodex_20170501_.thor"),
        ]
        .iter()
        .cloned()
        .collect();
        //Empty patch list
        let empty_thor_patch_list = patch_list_from_string("");
        assert_eq!(empty_thor_patch_list.len(), 0);
        // TODO(LinkZ): Ensure patch list is ordered by patch index
        // Regular patch list
        let thor_patch_list = patch_list_from_string(plist_content);
        assert_eq!(thor_patch_list.len(), expected_content.len());
        for patch_info in thor_patch_list {
            assert!(expected_content.contains_key(&patch_info.index));
            assert_eq!(patch_info.file_name, expected_content[&patch_info.index]);
        }
    }

    #[test]
    fn test_open_empty_container() {
        let thor_dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tests/thor");
        let thor_file_path = thor_dir_path.join("empty.thor");
        let thor_archive = ThorArchive::open(&thor_file_path).unwrap();
        assert_eq!(thor_archive.file_count(), 0);
        assert_eq!(thor_archive.target_grf_name(), "");
        assert!(thor_archive.use_grf_merging());
    }

    #[test]
    fn test_open_thor_container() {
        let thor_dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tests/thor");
        {
            let thor_file_path = thor_dir_path.join("dir1.thor");
            let mut thor_archive = ThorArchive::open(&thor_file_path).unwrap();
            assert_eq!(thor_archive.file_count(), 1);
            assert_eq!(thor_archive.target_grf_name(), "");
            assert!(!thor_archive.use_grf_merging());
            assert!(matches!(
                thor_archive.is_valid().unwrap_err(),
                GrufError::EntryNotFound
            ));
            let entry = thor_archive.get_entries().next().unwrap();
            assert_eq!(entry.offset, 52);
            assert_eq!(entry.size, 22528);
            assert_eq!(entry.size_compressed, 20136);
            assert!(!entry.is_removed);
            assert_eq!(entry.relative_path, "client.exe");
            let _content = thor_archive.read_file_content("client.exe").unwrap();
        }
        {
            let expected_content: HashMap<&str, usize> = [
                ("ASPLnchr.exe", 248568),
                ("savedata\\MiniPartyInfo.lua", 0),
                ("savedata\\OptionInfo.lua", 2703),
                ("savedata\\UserKeys.lua", 20),
            ]
            .iter()
            .cloned()
            .collect();
            let check_dir2_thor_entries = |thor: &mut ThorArchive<File>| {
                let file_entries: Vec<ThorFileEntry> = thor.get_entries().cloned().collect();
                for file_entry in file_entries {
                    let file_path: &str = &file_entry.relative_path[..];
                    assert!(expected_content.contains_key(file_path));
                    let expected_size = expected_content[file_path];
                    assert_eq!(file_entry.size, expected_size);
                    let _content = thor.read_file_content(file_path).unwrap();
                }
            };
            let thor_file_path = thor_dir_path.join("dir2.thor");
            let mut thor_archive = ThorArchive::open(&thor_file_path).unwrap();
            assert_eq!(thor_archive.file_count(), expected_content.len());
            assert_eq!(thor_archive.target_grf_name(), "");
            assert!(!thor_archive.use_grf_merging());
            assert!(matches!(
                thor_archive.is_valid().unwrap_err(),
                GrufError::EntryNotFound
            ));
            check_dir2_thor_entries(&mut thor_archive);
        }
        {
            let expected_content: HashMap<&str, usize> = [
                ("data.integrity", 63),
                (
                    "data\\texture\\\u{c0}\u{af}\u{c0}\u{fa}\u{c0}\u{ce}\u{c5}\u{cd}\u{c6}\u{e4}\u{c0}\u{cc}\u{bd}\u{ba}\\inventory\\icon_num.bmp",
                    560,
                ),
            ]
            .iter()
            .cloned()
            .collect();
            let check_tiny_thor_entries = |thor: &mut ThorArchive<File>| {
                let file_entries: Vec<ThorFileEntry> = thor.get_entries().cloned().collect();
                for file_entry in file_entries {
                    let file_path: &str = &file_entry.relative_path[..];
                    assert!(expected_content.contains_key(file_path));
                    let expected_size = expected_content[file_path];
                    assert_eq!(file_entry.size, expected_size);
                    let _content = thor.read_file_content(file_path).unwrap();
                }
            };
            let thor_file_path = thor_dir_path.join("tiny.thor");
            let mut thor_archive = ThorArchive::open(&thor_file_path).unwrap();
            assert_eq!(thor_archive.file_count(), expected_content.len());
            assert_eq!(thor_archive.target_grf_name(), "");
            assert!(thor_archive.use_grf_merging());
            check_tiny_thor_entries(&mut thor_archive);
            assert!(thor_archive.is_valid().unwrap());
        }
        {
            let thor_file_path = thor_dir_path.join("small.thor");
            let mut thor_archive = ThorArchive::open(&thor_file_path).unwrap();
            assert_eq!(thor_archive.file_count(), 16);
            assert_eq!(thor_archive.target_grf_name(), "data.grf");
            assert!(thor_archive.use_grf_merging());
            assert!(thor_archive.is_valid().unwrap());
        }
    }

    #[test]
    fn test_thor_bounds_and_integrity_hardening() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Cursor;

        // 1. Negative file_table_compressed_size in MultipleFiles table
        let mut neg_table = Vec::new();
        neg_table.extend_from_slice(THOR_HEADER_MAGIC);
        neg_table.push(0); // use_grf_merging
        neg_table.extend_from_slice(&1u32.to_le_bytes()); // file_count
        neg_table.extend_from_slice(&48i16.to_le_bytes()); // MultipleFiles
        neg_table.push(0); // target_grf_name_size
        neg_table.extend_from_slice(&(-1i32).to_le_bytes()); // file_table_compressed_size = -1
        neg_table.extend_from_slice(&64i32.to_le_bytes()); // file_table_offset
        assert!(ThorArchive::new(Cursor::new(neg_table)).is_err());

        // 2. Negative size_compressed in SingleFile entry
        let mut neg_single = Vec::new();
        neg_single.extend_from_slice(THOR_HEADER_MAGIC);
        neg_single.push(0);
        neg_single.extend_from_slice(&1u32.to_le_bytes());
        neg_single.extend_from_slice(&33i16.to_le_bytes()); // SingleFile
        neg_single.push(0); // target_grf_name_size
        neg_single.push(0); // single file table byte
        neg_single.extend_from_slice(&(-5i32).to_le_bytes()); // size_compressed = -5
        neg_single.extend_from_slice(&10i32.to_le_bytes()); // size
        neg_single.push(4);
        neg_single.extend_from_slice(b"a.tx");
        assert!(ThorArchive::new(Cursor::new(neg_single)).is_err());

        // 3. Declared file_table_compressed_size extending past EOF
        let mut oob_table = Vec::new();
        oob_table.extend_from_slice(THOR_HEADER_MAGIC);
        oob_table.push(0);
        oob_table.extend_from_slice(&1u32.to_le_bytes());
        oob_table.extend_from_slice(&48i16.to_le_bytes());
        oob_table.push(0);
        let offset = (oob_table.len() + 8) as i32;
        oob_table.extend_from_slice(&9999i32.to_le_bytes());
        oob_table.extend_from_slice(&offset.to_le_bytes());
        oob_table.extend_from_slice(&[0u8; 4]);
        assert!(ThorArchive::new(Cursor::new(oob_table)).is_err());

        // 4. Zlib bomb: compressed stream decompresses to more bytes than declared entry size
        let mut bomb_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        bomb_encoder.write_all(&[b'A'; 1024]).unwrap();
        let bomb_compressed = bomb_encoder.finish().unwrap();

        let mut bomb_thor = Vec::new();
        bomb_thor.extend_from_slice(THOR_HEADER_MAGIC);
        bomb_thor.push(0);
        bomb_thor.extend_from_slice(&1u32.to_le_bytes());
        bomb_thor.extend_from_slice(&33i16.to_le_bytes());
        bomb_thor.push(0);
        bomb_thor.push(0);
        bomb_thor.extend_from_slice(&(bomb_compressed.len() as i32).to_le_bytes());
        bomb_thor.extend_from_slice(&4i32.to_le_bytes()); // claim only 4 bytes!
        bomb_thor.push(8);
        bomb_thor.extend_from_slice(b"bomb.txt");
        bomb_thor.extend_from_slice(&bomb_compressed);
        let mut bomb_arch = ThorArchive::new(Cursor::new(bomb_thor)).unwrap();
        assert!(bomb_arch.read_file_content("bomb.txt").is_err());

        // 5. Completeness check in is_valid(): entry omitted from data.integrity must return Ok(false)
        let mut payload_enc = ZlibEncoder::new(Vec::new(), Compression::default());
        payload_enc.write_all(b"verified").unwrap();
        let c1 = payload_enc.finish().unwrap();

        let mut smuggled_enc = ZlibEncoder::new(Vec::new(), Compression::default());
        smuggled_enc.write_all(b"evil").unwrap();
        let c2 = smuggled_enc.finish().unwrap();

        let integrity_line = format!("ok.txt=0x{:08x}\r\n", crc32::checksum_ieee(b"verified"));
        let mut integ_enc = ZlibEncoder::new(Vec::new(), Compression::default());
        integ_enc.write_all(integrity_line.as_bytes()).unwrap();
        let c_integ = integ_enc.finish().unwrap();

        let mut buf = vec![0u8; THOR_HEADER_MAGIC.len() + 8 + 8];
        let off1 = buf.len() as u32;
        buf.extend_from_slice(&c1);
        let off2 = buf.len() as u32;
        buf.extend_from_slice(&c2);
        let off_integ = buf.len() as u32;
        buf.extend_from_slice(&c_integ);

        let mut raw_tbl = Vec::new();
        for (name, off, csz, sz) in [
            ("ok.txt", off1, c1.len(), 8usize),
            ("smuggled.exe", off2, c2.len(), 4usize),
            (INTEGRITY_FILE_NAME, off_integ, c_integ.len(), integrity_line.len()),
        ] {
            raw_tbl.push(name.len() as u8);
            raw_tbl.extend_from_slice(name.as_bytes());
            raw_tbl.push(0); // flags
            raw_tbl.extend_from_slice(&off.to_le_bytes());
            raw_tbl.extend_from_slice(&(csz as i32).to_le_bytes());
            raw_tbl.extend_from_slice(&(sz as i32).to_le_bytes());
        }
        let mut tbl_enc = ZlibEncoder::new(Vec::new(), Compression::default());
        tbl_enc.write_all(&raw_tbl).unwrap();
        let c_tbl = tbl_enc.finish().unwrap();
        let tbl_off = buf.len() as i32;
        buf.extend_from_slice(&c_tbl);

        let mut hdr = Vec::new();
        hdr.extend_from_slice(THOR_HEADER_MAGIC);
        hdr.push(0);
        hdr.extend_from_slice(&3u32.to_le_bytes());
        hdr.extend_from_slice(&48i16.to_le_bytes());
        hdr.push(0);
        hdr.extend_from_slice(&(c_tbl.len() as i32).to_le_bytes());
        hdr.extend_from_slice(&tbl_off.to_le_bytes());
        buf[..hdr.len()].copy_from_slice(&hdr);

        let mut smuggled_arch = ThorArchive::new(Cursor::new(buf)).unwrap();
        assert!(!smuggled_arch.is_valid().unwrap());
    }
}
