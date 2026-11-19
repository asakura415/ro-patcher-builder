use std::boxed::Box;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::str;

use crate::grf::crypto::{decrypt_file_content, decrypt_file_name};
use crate::{GrufError, Result};
use encoding::label::encoding_from_whatwg_label;
use encoding::DecoderTrap;
use flate2::read::ZlibDecoder;
use nom::error::ErrorKind;
use nom::number::complete::{le_i32, le_u32, le_u8};
use nom::*;

pub const GRF_HEADER_MAGIC: &str = "Master of Magic\0";
// Packed structs' sizes in bytes
pub const GRF_HEADER_SIZE: usize = GRF_HEADER_MAGIC.len() + 0x1E;
const GRF_TABLE_INFO2_SIZE: usize = 2 * std::mem::size_of::<u32>();

// Defensive bounds for untrusted GRF headers/tables/entries
const MAX_GRF_TABLE_COMPRESSED_SIZE: usize = 128 * 1024 * 1024;
const MAX_GRF_TABLE_DECOMPRESSED_SIZE: usize = 512 * 1024 * 1024;
const MAX_GRF_ENTRY_SIZE: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub struct GrfArchive {
    obj: Box<File>,
    container: GrfContainer,
}

impl GrfArchive {
    /// Create a new archive with the underlying object as the reader.
    pub fn open<P: AsRef<Path>>(grf_path: P) -> Result<Self> {
        let mut file = File::open(grf_path)?;
        let file_len = file.metadata()?.len();
        let mut grf_header_buf = [0; GRF_HEADER_SIZE];
        file.read_exact(&mut grf_header_buf)?;
        let (parser_output, grf_header) = parse_grf_header(&grf_header_buf)
            .map_err(|_| GrufError::parsing_error("Failed to parse archive (header)"))?;

        match grf_header.version_major {
            2 => {
                let table_pos = (GRF_HEADER_SIZE as u64)
                    .checked_add(grf_header.file_table_offset)
                    .ok_or_else(|| GrufError::parsing_error("GRF table offset overflow"))?;
                let table_data_start = table_pos
                    .checked_add(GRF_TABLE_INFO2_SIZE as u64)
                    .ok_or_else(|| GrufError::parsing_error("GRF table info offset overflow"))?;
                if table_data_start > file_len {
                    return Err(GrufError::parsing_error(
                        "GRF file table info extends past EOF",
                    ));
                }

                let mut table_info_buf = [0; GRF_TABLE_INFO2_SIZE];
                file.seek(SeekFrom::Start(table_pos))?;
                file.read_exact(&mut table_info_buf)?;
                let (_parser_output, grf_table_info) = parse_grf_table_info_200(&table_info_buf)
                    .map_err(|_| {
                        GrufError::parsing_error("Failed to parse archive (table info)")
                    })?;
                if grf_table_info.table_size_compressed == 0
                    || grf_table_info.table_size == 0
                    || grf_header.file_count == 0
                {
                    return Ok(Self {
                        obj: Box::new(file),
                        container: GrfContainer {
                            header: grf_header,
                            table_info: GrfTableInfo::Compressed(grf_table_info),
                            entries: HashMap::new(),
                        },
                    });
                }
                if grf_table_info.table_size_compressed > MAX_GRF_TABLE_COMPRESSED_SIZE {
                    return Err(GrufError::parsing_error(
                        "GRF compressed file table exceeds maximum allowed size",
                    ));
                }
                let table_end = table_data_start
                    .checked_add(grf_table_info.table_size_compressed as u64)
                    .ok_or_else(|| GrufError::parsing_error("GRF table size overflow"))?;
                if table_end > file_len {
                    return Err(GrufError::parsing_error(
                        "GRF compressed file table extends past EOF",
                    ));
                }

                // Decompress the table with zlib (bounded)
                let mut compressed_table = vec![0u8; grf_table_info.table_size_compressed];
                file.read_exact(&mut compressed_table)?;
                let decoder = ZlibDecoder::new(compressed_table.as_slice());
                let mut decompressed_table = Vec::new();
                let decompressed_size = decoder
                    .take(MAX_GRF_TABLE_DECOMPRESSED_SIZE as u64 + 1)
                    .read_to_end(&mut decompressed_table)
                    .map_err(|e| {
                        GrufError::ParsingError(format!("Failed to decompress file table: {}", e))
                    })?;
                if decompressed_size > MAX_GRF_TABLE_DECOMPRESSED_SIZE {
                    return Err(GrufError::parsing_error(
                        "GRF decompressed file table exceeds maximum allowed size",
                    ));
                }
                // Parse entries
                let (_output, entries) = parse_grf_file_entries_200(
                    decompressed_table.as_slice(),
                    grf_header.file_count,
                )
                .map_err(|_| GrufError::parsing_error("Failed to parse file table"))?;
                Ok(Self {
                    obj: Box::new(file),
                    container: GrfContainer {
                        header: grf_header,
                        table_info: GrfTableInfo::Compressed(grf_table_info),
                        entries,
                    },
                })
            }
            1 => {
                // Only versions 1.1, 1.2 and 1.3 are supported
                if grf_header.version_minor < 1 || grf_header.version_minor > 3 {
                    return Err(GrufError::parsing_error("Unsupported archive version"));
                }
                let table_size = parser_output.len();
                if table_size == 0 || grf_header.file_count <= 1 {
                    return Ok(Self {
                        obj: Box::new(file),
                        container: GrfContainer {
                            header: grf_header,
                            table_info: GrfTableInfo::Uncompressed(GrfTableInfo1 { table_size }),
                            entries: HashMap::new(),
                        },
                    });
                }
                let table_offset = usize::try_from(grf_header.file_table_offset).map_err(|_| {
                    GrufError::parsing_error("GRF 1.x table offset out of range")
                })?;
                if table_offset > parser_output.len() {
                    return Err(GrufError::parsing_error(
                        "GRF 1.x table offset extends past buffer",
                    ));
                }
                // Parse entries
                let (_parser_output, entries) = parse_grf_file_entries_101(
                    &parser_output[table_offset..],
                    grf_header.file_count,
                )
                .map_err(|_| GrufError::parsing_error("Failed to parse file table"))?;

                Ok(Self {
                    obj: Box::new(file),
                    container: GrfContainer {
                        header: grf_header,
                        table_info: GrfTableInfo::Uncompressed(GrfTableInfo1 { table_size }),
                        entries,
                    },
                })
            }
            _ => Err(GrufError::parsing_error("Unsupported archive version")),
        }
    }

    pub fn file_count(&self) -> usize {
        self.container.header.file_count
    }

    pub fn version_major(&self) -> u32 {
        self.container.header.version_major
    }

    pub fn version_minor(&self) -> u32 {
        self.container.header.version_minor
    }

    pub fn get_entry_raw_data<S: AsRef<str> + Hash>(&mut self, file_path: S) -> Result<Vec<u8>> {
        let file_entry = self
            .get_file_entry(file_path)
            .ok_or(GrufError::EntryNotFound)?
            .clone();
        if file_entry.size == 0 {
            return Ok(vec![]);
        }
        if file_entry.size_compressed_aligned > MAX_GRF_ENTRY_SIZE
            || file_entry.size > MAX_GRF_ENTRY_SIZE
        {
            return Err(GrufError::parsing_error(
                "GRF entry size exceeds maximum allowed limit",
            ));
        }

        let file_len = self.obj.metadata()?.len();
        let entry_end = file_entry
            .offset
            .checked_add(file_entry.size_compressed_aligned as u64)
            .ok_or_else(|| GrufError::parsing_error("GRF entry offset overflow"))?;
        if entry_end > file_len {
            return Err(GrufError::parsing_error(
                "GRF entry extends past end of archive",
            ));
        }

        self.obj.seek(SeekFrom::Start(file_entry.offset))?;
        let mut content = vec![0u8; file_entry.size_compressed_aligned];
        self.obj.read_exact(&mut content)?;
        Ok(content)
    }

    pub fn read_file_content<S: AsRef<str> + Hash>(&mut self, file_path: S) -> Result<Vec<u8>> {
        let file_entry = self
            .get_file_entry(file_path)
            .ok_or(GrufError::EntryNotFound)?
            .clone();
        if file_entry.size == 0 {
            return Ok(vec![]);
        }
        if file_entry.size_compressed_aligned > MAX_GRF_ENTRY_SIZE
            || file_entry.size > MAX_GRF_ENTRY_SIZE
        {
            return Err(GrufError::parsing_error(
                "GRF entry size exceeds maximum allowed limit",
            ));
        }

        let file_len = self.obj.metadata()?.len();
        let entry_end = file_entry
            .offset
            .checked_add(file_entry.size_compressed_aligned as u64)
            .ok_or_else(|| GrufError::parsing_error("GRF entry offset overflow"))?;
        if entry_end > file_len {
            return Err(GrufError::parsing_error(
                "GRF entry extends past end of archive",
            ));
        }

        self.obj.seek(SeekFrom::Start(file_entry.offset))?;
        let mut content = vec![0u8; file_entry.size_compressed_aligned];
        self.obj.read_exact(&mut content)?;
        match file_entry.encryption {
            GrfFileEncryption::Unencrypted => {}
            GrfFileEncryption::Encrypted(cycle) => {
                decrypt_file_content(&mut content, cycle);
            }
        }
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

    pub fn contains_file<S: AsRef<str> + Hash>(&self, file_path: S) -> bool {
        self.container.entries.contains_key(file_path.as_ref())
    }

    pub fn get_file_entry<S: AsRef<str> + Hash>(&self, file_path: S) -> Option<&GrfFileEntry> {
        self.container.entries.get(file_path.as_ref())
    }

    pub fn get_entries(&self) -> impl Iterator<Item = &'_ GrfFileEntry> {
        self.container.entries.values()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct GrfContainer {
    pub header: GrfHeader,
    pub table_info: GrfTableInfo,
    pub entries: HashMap<String, GrfFileEntry>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GrfHeader {
    pub key: [u8; 14],
    pub file_table_offset: u64,
    pub seed: i32,
    pub file_count: usize,
    pub version_major: u32,
    pub version_minor: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum GrfTableInfo {
    Uncompressed(GrfTableInfo1),
    Compressed(GrfTableInfo2),
}

#[derive(Debug, PartialEq, Eq)]
struct GrfTableInfo1 {
    pub table_size: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct GrfTableInfo2 {
    pub table_size_compressed: usize,
    pub table_size: usize,
}

#[derive(Debug, Clone, Eq)]
pub struct GrfFileEntry {
    pub relative_path: String,
    pub size_compressed: usize,
    pub size_compressed_aligned: usize,
    pub size: usize,
    pub entry_type: u8,
    pub offset: u64,
    pub encryption: GrfFileEncryption,
}

impl Hash for GrfFileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.relative_path.hash(state);
    }
}

impl PartialEq for GrfFileEntry {
    fn eq(&self, other: &GrfFileEntry) -> bool {
        self.relative_path == other.relative_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrfFileEncryption {
    Unencrypted,
    Encrypted(usize), // Contains the cycle as usize
}

fn parse_grf_header(input: &[u8]) -> IResult<&[u8], GrfHeader> {
    let (input, _) = tag!(input, GRF_HEADER_MAGIC)?;
    let (input, key) = take!(input, 14usize)?;
    let (input, file_table_offset) = le_u32(input)?;
    let (input, seed) = le_i32(input)?;
    let (input, v_files_count) = le_i32(input)?;
    let (input, version) = le_u32(input)?;
    let file_count = v_files_count
        .checked_sub(seed)
        .and_then(|v| v.checked_sub(7))
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| nom::Err::Failure((input, ErrorKind::Verify)))?;

    Ok((
        input,
        GrfHeader {
            key: key.try_into().unwrap(),
            file_table_offset: file_table_offset as u64,
            seed,
            file_count,
            version_major: (version >> 8) & 0xFF,
            version_minor: version & 0xFF,
        },
    ))
}

named!(parse_grf_table_info_200<&[u8], GrfTableInfo2>,
    do_parse!(
        table_size_compressed: le_u32
            >> table_size: le_u32
            >> (GrfTableInfo2 {
                table_size_compressed: table_size_compressed as usize,
                table_size: table_size as usize,
            }
    )
));

fn string_from_win_1252(v: &[u8]) -> Result<String> {
    if let Some(decoder) = encoding_from_whatwg_label("windows-1252") {
        if let Ok(s) = decoder.decode(v, DecoderTrap::Strict) {
            return Ok(s);
        }
    }
    Ok(v.iter().map(|&b| char::from(b)).collect())
}

macro_rules! take_obfuscated_name_101 (
    ( $i:expr, $size:expr ) => (
        {
            let input: &[u8] = $i;
            let (parser_output, file_name_bytes) = map_res!(input, take!($size), decrypt_file_name)?;
            match string_from_win_1252(file_name_bytes.as_slice()) {
                Ok(v) => Ok((parser_output , v)),
                Err(_) => Err(nom::Err::Failure((parser_output, ErrorKind::AlphaNumeric))),
            }
        }
     );
);

fn determine_file_encryption_101(file_name: &str, size_compressed: usize) -> GrfFileEncryption {
    const SPECIAL_EXTENSIONS: [&str; 4] = [".gnd", ".gat", ".act", ".str"];
    if SPECIAL_EXTENSIONS.iter().any(|ext| file_name.ends_with(ext)) {
        GrfFileEncryption::Encrypted(0)
    } else {
        GrfFileEncryption::Encrypted(digit_count(size_compressed))
    }
}

/// Counts decimal digits without multiplication overflow
fn digit_count(mut n: usize) -> usize {
    let mut result = 1;
    while n >= 10 {
        n /= 10;
        result += 1;
    }
    result
}

// Parses file table entries for GRF 1.1, 1.2 and 1.3
fn parse_grf_file_entry_101(input: &[u8]) -> IResult<&[u8], GrfFileEntry> {
    let (input, path_size_padded) = le_u32(input)?;
    let path_len = path_size_padded
        .checked_sub(6)
        .ok_or_else(|| nom::Err::Failure((input, ErrorKind::Verify)))?;
    let (input, _) = take!(input, 2usize)?; // Null chars
    let (input, relative_path) = take_obfuscated_name_101!(input, path_len)?;
    let (input, _) = take!(input, 4usize)?; // Null chars
    let (input, size_tot_enc) = le_u32(input)?;
    let (input, size_compressed_aligned_enc) = le_u32(input)?;
    let (input, size) = le_u32(input)?;
    let (input, entry_type) = le_u8(input)?;
    let (input, offset) = le_u32(input)?;

    let size_compressed = size_tot_enc
        .checked_sub(size)
        .and_then(|v| v.checked_sub(0x02CB))
        .ok_or_else(|| nom::Err::Failure((input, ErrorKind::Verify)))? as usize;
    let size_compressed_aligned = size_compressed_aligned_enc
        .checked_sub(0x92CB)
        .ok_or_else(|| nom::Err::Failure((input, ErrorKind::Verify)))? as usize;

    Ok((
        input,
        GrfFileEntry {
            size_compressed,
            size_compressed_aligned,
            size: size as usize,
            entry_type,
            offset: GRF_HEADER_SIZE as u64 + offset as u64,
            encryption: determine_file_encryption_101(&relative_path, size_compressed),
            relative_path,
        },
    ))
}

// Parses file table entries for GRF 2.0
named!(parse_grf_file_entry_200<&[u8], GrfFileEntry>,
    do_parse!(
        relative_path: map_res!(take_while!(|ch: u8| ch != 0), string_from_win_1252)
            >> take!(1) // Null char terminator
            >> size_compressed: le_u32
            >> size_compressed_aligned: le_u32
            >> size: le_u32
            >> entry_type: le_u8
            >> offset: le_u32
            >> (GrfFileEntry {
                relative_path,
                size_compressed: size_compressed as usize,
                size_compressed_aligned: size_compressed_aligned as usize,
                size: size as usize,
                entry_type,
                offset: GRF_HEADER_SIZE as u64 + offset as u64,
                encryption: GrfFileEncryption::Unencrypted,
            }
        )
    )
);

named_args!(parse_grf_file_entries_101(files_count: usize)<&[u8], HashMap<String, GrfFileEntry>>,
fold_many_m_n!(1, files_count - 1, parse_grf_file_entry_101, HashMap::new(), |mut acc: HashMap<_, _>, item| {
        acc.insert(item.relative_path.clone(), item);
        acc
    })
);

named_args!(parse_grf_file_entries_200(files_count: usize)<&[u8], HashMap<String, GrfFileEntry>>,
fold_many_m_n!(1, files_count, parse_grf_file_entry_200, HashMap::new(), |mut acc: HashMap<_, _>, item| {
        acc.insert(item.relative_path.clone(), item);
        acc
    })
);

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;
    use std::path::PathBuf;
    use twox_hash::XxHash64;

    #[test]
    fn test_open_grf_container() {
        let grf_dir_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tests/grf");
        let expected_content: HashMap<&str, (usize, u64)> = [
            (
                "data\\06guild_r.gat",
                (800014, u64::from_be_bytes(hex!("b740a01075ce37f2"))),
            ),
            (
                "data\\06guild_r.gnd",
                (454622, u64::from_be_bytes(hex!("213f0c61fff67856"))),
            ),
            (
                "data\\06guild_r.rsw",
                (69798, u64::from_be_bytes(hex!("519d99273b1b4d38"))),
            ),
            (
                "data\\sprite\\\u{B8}\u{F3}\u{BD}\u{BA}\u{C5}\u{CD}\\high_orc.act",
                (491076, u64::from_be_bytes(hex!("5f26d5f20679a2af"))),
            ),
            (
                "data\\sprite\\\u{B8}\u{F3}\u{BD}\u{BA}\u{C5}\u{CD}\\high_orc.spr",
                (250592, u64::from_be_bytes(hex!("b8356a4d4517df6e"))),
            ),
            (
                "data\\texture\\chdesk-side1.bmp",
                (33844, u64::from_be_bytes(hex!("b4bc113b3ca8a655"))),
            ),
            (
                "data\\texture\\chdesk-side2.bmp",
                (33844, u64::from_be_bytes(hex!("c81a827857725179"))),
            ),
            (
                "data\\texture\\chdesk-side3.bmp",
                (17460, u64::from_be_bytes(hex!("2c796a702a93682f"))),
            ),
        ]
        .iter()
        .cloned()
        .collect();
        let check_small_grf_entries = |grf: &mut GrfArchive| {
            let file_entries: Vec<GrfFileEntry> = grf.get_entries().cloned().collect();
            for file_entry in file_entries {
                let file_path: &str = &file_entry.relative_path[..];
                assert!(expected_content.contains_key(file_path));
                let (expected_size, expected_hash) = expected_content[file_path];
                assert_eq!(file_entry.size, expected_size);
                // Size check
                let file_content = grf.read_file_content(file_path).unwrap();
                assert_eq!(file_content.len(), expected_size);
                // Hash check
                let mut hasher = XxHash64::default();
                hasher.write(file_content.as_slice());
                assert_eq!(hasher.finish(), expected_hash);
            }
        };
        {
            let grf_path = grf_dir_path.join("200-empty.grf");
            let grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 0);
            assert_eq!(grf.version_major(), 2);
            assert_eq!(grf.version_minor(), 0);
        }

        {
            let grf_path = grf_dir_path.join("200-small.grf");
            let mut grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 8);
            assert_eq!(grf.version_major(), 2);
            assert_eq!(grf.version_minor(), 0);
            check_small_grf_entries(&mut grf);
        }

        {
            let grf_path = grf_dir_path.join("103-empty.grf");
            let grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 0);
            assert_eq!(grf.version_major(), 1);
            assert_eq!(grf.version_minor(), 3);
        }

        {
            let grf_path = grf_dir_path.join("103-small.grf");
            let mut grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 8);
            assert_eq!(grf.version_major(), 1);
            assert_eq!(grf.version_minor(), 3);
            check_small_grf_entries(&mut grf);
        }

        {
            let grf_path = grf_dir_path.join("102-empty.grf");
            let grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 0);
            assert_eq!(grf.version_major(), 1);
            assert_eq!(grf.version_minor(), 2);
        }

        {
            let grf_path = grf_dir_path.join("102-small.grf");
            let mut grf = GrfArchive::open(&grf_path).unwrap();
            assert_eq!(grf.file_count(), 8);
            assert_eq!(grf.version_major(), 1);
            assert_eq!(grf.version_minor(), 2);
            check_small_grf_entries(&mut grf);
        }
    }

    #[test]
    fn test_digit_count() {
        assert_eq!(1, digit_count(0));
        assert_eq!(1, digit_count(8));
        assert_eq!(2, digit_count(13));
        assert_eq!(2, digit_count(99));
        assert_eq!(3, digit_count(100));
        assert_eq!(8, digit_count(87654321));
        assert!(digit_count(usize::MAX) >= 10);
    }

    #[test]
    fn test_grf_bounds_and_utf8_safety() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // 1. Negative file_count underflow in GRF header
        let mut hdr = Vec::new();
        hdr.extend_from_slice(GRF_HEADER_MAGIC.as_bytes());
        hdr.extend_from_slice(&[0u8; 14]);
        hdr.extend_from_slice(&0u32.to_le_bytes()); // file_table_offset
        hdr.extend_from_slice(&10i32.to_le_bytes()); // seed
        hdr.extend_from_slice(&0i32.to_le_bytes()); // v_files_count (0 - 10 - 7 < 0)
        hdr.extend_from_slice(&0x0200u32.to_le_bytes()); // version 2.0
        assert!(parse_grf_header(&hdr).is_err());

        // 2. Oversized table_size_compressed > file length
        let mut tmp = NamedTempFile::new().unwrap();
        let mut grf_buf = Vec::new();
        grf_buf.extend_from_slice(GRF_HEADER_MAGIC.as_bytes());
        grf_buf.extend_from_slice(&[0u8; 14]);
        grf_buf.extend_from_slice(&0u32.to_le_bytes()); // file_table_offset = 0 (starts right after header)
        grf_buf.extend_from_slice(&0i32.to_le_bytes()); // seed = 0
        grf_buf.extend_from_slice(&8i32.to_le_bytes()); // v_files_count = 8 => 1 file
        grf_buf.extend_from_slice(&0x0200u32.to_le_bytes()); // version 2.0
        grf_buf.extend_from_slice(&999_999u32.to_le_bytes()); // table_size_compressed > EOF
        grf_buf.extend_from_slice(&100u32.to_le_bytes()); // table_size
        tmp.write_all(&grf_buf).unwrap();
        assert!(GrfArchive::open(tmp.path()).is_err());

        // 3. determine_file_encryption_101 with multi-byte UTF-8 suffix must not panic
        assert_eq!(
            determine_file_encryption_101("a\u{AC00}\u{B098}", 123),
            GrfFileEncryption::Encrypted(3)
        );
        assert_eq!(
            determine_file_encryption_101("map\u{AC00}.gnd", 123),
            GrfFileEncryption::Encrypted(0)
        );
    }
}
