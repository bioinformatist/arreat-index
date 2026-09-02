#![forbid(unsafe_code)]

#[cfg(any(target_os = "linux", test))]
use std::collections::{BTreeMap, BTreeSet};
use std::{
    io::{BufRead, Write},
    path::Path,
};

use serde::Serialize;
use thiserror::Error;

#[cfg(any(target_os = "linux", test))]
const IMAGE_FILE_DLL: u16 = 0x2000;
#[cfg(any(target_os = "linux", test))]
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
#[cfg(any(target_os = "linux", test))]
const X86_64_MACHINE: u16 = 0x8664;
#[cfg(any(target_os = "linux", test))]
const PE32_PLUS_MAGIC: u16 = 0x020b;
#[cfg(any(target_os = "linux", test))]
const HOVER_RECORD_SIZE: usize = 12;
#[cfg(any(target_os = "linux", test))]
const UNIT_HASH_PATTERN: &[u8] = &[0x48, 0x03, 0xc7, 0x49, 0x8b, 0x8c, 0xc6];

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PeSection {
    name: [u8; 8],
    rva: u32,
    size: u32,
    executable: bool,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PeImage {
    machine: u16,
    size_of_image: u32,
    sections: Vec<PeSection>,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
enum PeError {
    #[error("invalid DOS header")]
    Dos,
    #[error("invalid PE header")]
    Signature,
    #[error("unsupported PE image")]
    Unsupported,
    #[error("PE image is a DLL")]
    Dll,
    #[error("truncated PE headers")]
    Truncated,
    #[error("invalid PE arithmetic")]
    Arithmetic,
    #[error("invalid PE section range")]
    SectionRange,
}

#[cfg(any(target_os = "linux", test))]
fn parse_pe(bytes: &[u8]) -> Result<PeImage, PeError> {
    if read_bytes(bytes, 0, 2)? != b"MZ" {
        return Err(PeError::Dos);
    }
    let pe_offset = usize::try_from(read_u32(bytes, 0x3c)?).map_err(|_| PeError::Arithmetic)?;
    if read_bytes(bytes, pe_offset, 4)? != b"PE\0\0" {
        return Err(PeError::Signature);
    }

    let coff = pe_offset.checked_add(4).ok_or(PeError::Arithmetic)?;
    let machine = read_u16(bytes, coff)?;
    let section_count = usize::from(read_u16(bytes, checked_add(coff, 2)?)?);
    let optional_size = usize::from(read_u16(bytes, checked_add(coff, 16)?)?);
    let characteristics = read_u16(bytes, checked_add(coff, 18)?)?;
    if machine != X86_64_MACHINE {
        return Err(PeError::Unsupported);
    }
    if characteristics & IMAGE_FILE_DLL != 0 {
        return Err(PeError::Dll);
    }

    let optional = checked_add(coff, 20)?;
    if optional_size < 60 {
        return Err(PeError::Unsupported);
    }
    if read_u16(bytes, optional)? != PE32_PLUS_MAGIC {
        return Err(PeError::Unsupported);
    }
    let size_of_image = read_u32(bytes, checked_add(optional, 56)?)?;
    if size_of_image == 0 {
        return Err(PeError::SectionRange);
    }
    let section_table = optional
        .checked_add(optional_size)
        .ok_or(PeError::Arithmetic)?;
    let section_bytes = section_count.checked_mul(40).ok_or(PeError::Arithmetic)?;
    read_bytes(bytes, section_table, section_bytes)?;

    let mut sections = Vec::with_capacity(section_count);
    for index in 0..section_count {
        let row = section_table
            .checked_add(index.checked_mul(40).ok_or(PeError::Arithmetic)?)
            .ok_or(PeError::Arithmetic)?;
        let mut name = [0; 8];
        name.copy_from_slice(read_bytes(bytes, row, 8)?);
        let size = read_u32(bytes, checked_add(row, 8)?)?;
        let rva = read_u32(bytes, checked_add(row, 12)?)?;
        let end = rva.checked_add(size).ok_or(PeError::Arithmetic)?;
        if end > size_of_image {
            return Err(PeError::SectionRange);
        }
        let flags = read_u32(bytes, checked_add(row, 36)?)?;
        sections.push(PeSection {
            name,
            rva,
            size,
            executable: flags & IMAGE_SCN_MEM_EXECUTE != 0,
        });
    }

    Ok(PeImage {
        machine,
        size_of_image,
        sections,
    })
}

#[cfg(any(target_os = "linux", test))]
fn checked_add(left: usize, right: usize) -> Result<usize, PeError> {
    left.checked_add(right).ok_or(PeError::Arithmetic)
}

#[cfg(any(target_os = "linux", test))]
fn read_bytes(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], PeError> {
    let end = offset.checked_add(len).ok_or(PeError::Arithmetic)?;
    bytes.get(offset..end).ok_or(PeError::Truncated)
}

#[cfg(any(target_os = "linux", test))]
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PeError> {
    let value: [u8; 2] = read_bytes(bytes, offset, 2)?
        .try_into()
        .map_err(|_| PeError::Truncated)?;
    Ok(u16::from_le_bytes(value))
}

#[cfg(any(target_os = "linux", test))]
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PeError> {
    let value: [u8; 4] = read_bytes(bytes, offset, 4)?
        .try_into()
        .map_err(|_| PeError::Truncated)?;
    Ok(u32::from_le_bytes(value))
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadableRange {
    start: u64,
    end: u64,
}

#[cfg(any(target_os = "linux", test))]
fn range_is_covered(ranges: &[ReadableRange], start: u64, len: u64) -> bool {
    let Some(end) = start.checked_add(len) else {
        return false;
    };
    let mut cursor = start;
    let mut ordered = ranges.to_vec();
    ordered.sort_unstable_by_key(|range| range.start);
    for range in ordered {
        if range.end <= cursor || range.start > cursor {
            continue;
        }
        cursor = cursor.max(range.end);
        if cursor >= end {
            return true;
        }
    }
    cursor >= end
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct SectionBytes {
    name: [u8; 8],
    rva: u32,
    declared_size: u32,
    bytes: Vec<u8>,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolver {
    DirectDisplacement,
    RipRelative,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug)]
struct Pattern {
    name: &'static str,
    bytes: &'static [Option<u8>],
    resolver: Resolver,
}

#[cfg(any(target_os = "linux", test))]
const LAST_HOVER_V3: &[Option<u8>] = &[
    Some(0xc6),
    Some(0x84),
    Some(0xc2),
    None,
    None,
    None,
    None,
    None,
    Some(0x48),
    Some(0x8b),
    Some(0x74),
    Some(0x24),
];
#[cfg(any(target_os = "linux", test))]
const MOUSE_V1: &[Option<u8>] = &[
    Some(0x48),
    Some(0x8d),
    Some(0x3d),
    None,
    None,
    None,
    None,
    Some(0xbb),
    None,
    None,
    None,
    None,
    Some(0x48),
    Some(0x8b),
    Some(0xcf),
    Some(0xe8),
    None,
    None,
    None,
    None,
    Some(0x48),
    Some(0x83),
    Some(0xc7),
    Some(0x10),
];
#[cfg(any(target_os = "linux", test))]
fn patterns() -> [Pattern; 4] {
    [
        Pattern {
            name: "last-hover-v3",
            bytes: LAST_HOVER_V3,
            resolver: Resolver::DirectDisplacement,
        },
        Pattern {
            name: "mouse-v1",
            bytes: MOUSE_V1,
            resolver: Resolver::RipRelative,
        },
        Pattern {
            name: "mouse-short",
            bytes: &MOUSE_V1[..20],
            resolver: Resolver::RipRelative,
        },
        Pattern {
            name: "mouse-lea",
            bytes: &MOUSE_V1[..15],
            resolver: Resolver::RipRelative,
        },
    ]
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PatternCount {
    name: &'static str,
    hits: usize,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct Candidate {
    target: u64,
    pattern_names: Vec<&'static str>,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ScanResult {
    patterns: Vec<PatternCount>,
    unit_hash_hits: usize,
    candidates: Vec<Candidate>,
}

#[cfg(any(target_os = "linux", test))]
fn scan_patterns(
    module_base: u64,
    sections: &[SectionBytes],
    mut read_record: impl FnMut(u64, &mut [u8]) -> bool,
) -> ScanResult {
    let patterns = patterns();
    let mut resolved = BTreeMap::<u64, BTreeSet<&'static str>>::new();
    let mut pattern_counts = Vec::with_capacity(patterns.len());

    for pattern in &patterns {
        let mut hits = 0;
        for section in sections {
            for offset in wildcard_matches(&section.bytes, pattern.bytes) {
                hits += 1;
                let Some(match_rva) =
                    u64::from(section.rva).checked_add(u64::try_from(offset).unwrap_or(u64::MAX))
                else {
                    continue;
                };
                let Some(target) = resolve_target(
                    module_base,
                    match_rva,
                    &section.bytes[offset..],
                    pattern.resolver,
                ) else {
                    continue;
                };
                resolved.entry(target).or_default().insert(pattern.name);
            }
        }
        pattern_counts.push(PatternCount {
            name: pattern.name,
            hits,
        });
    }

    let candidates = resolved
        .into_iter()
        .filter_map(|(target, names)| {
            let mut record = [0; HOVER_RECORD_SIZE];
            read_record(target, &mut record).then_some(Candidate {
                target,
                pattern_names: patterns
                    .iter()
                    .filter_map(|pattern| names.contains(pattern.name).then_some(pattern.name))
                    .collect(),
            })
        })
        .collect();

    ScanResult {
        patterns: pattern_counts,
        unit_hash_hits: sections
            .iter()
            .map(|section| literal_match_count(&section.bytes, UNIT_HASH_PATTERN))
            .sum(),
        candidates,
    }
}

#[cfg(any(target_os = "linux", test))]
fn wildcard_matches(haystack: &[u8], needle: &[Option<u8>]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, window)| {
            window
                .iter()
                .zip(needle)
                .all(|(actual, expected)| expected.is_none_or(|byte| byte == *actual))
                .then_some(offset)
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn literal_match_count(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || needle.len() > haystack.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[cfg(any(target_os = "linux", test))]
fn resolve_target(
    module_base: u64,
    match_rva: u64,
    match_bytes: &[u8],
    resolver: Resolver,
) -> Option<u64> {
    let displacement = i32::from_le_bytes(match_bytes.get(3..7)?.try_into().ok()?);
    let origin = match resolver {
        Resolver::DirectDisplacement => module_base.checked_sub(1)?,
        Resolver::RipRelative => module_base.checked_add(match_rva)?.checked_add(7)?,
    };
    checked_add_signed(origin, displacement)
}

#[cfg(any(target_os = "linux", test))]
fn checked_add_signed(value: u64, displacement: i32) -> Option<u64> {
    let result = i128::from(value) + i128::from(displacement);
    u64::try_from(result).ok()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        fs::{self, File},
        io::Read,
        os::unix::fs::FileExt,
        path::Path,
    };

    use sha2::{Digest, Sha256};

    use super::{
        HOVER_RECORD_SIZE, PeImage, ProbeError, ReadableRange, ScanResult, SectionBytes, parse_pe,
        range_is_covered, read_u16, read_u32, scan_patterns,
    };

    const PROC_ROOT: &str = "/proc";

    trait MemoryReader {
        fn read_once(&self, address: u64, destination: &mut [u8]) -> bool;
    }

    struct FileMemory(File);

    impl MemoryReader for FileMemory {
        fn read_once(&self, address: u64, destination: &mut [u8]) -> bool {
            matches!(self.0.read_at(destination, address), Ok(read) if read == destination.len())
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct MapEntry {
        range: ReadableRange,
        readable: bool,
        offset: u64,
        anonymous: bool,
    }

    #[derive(Clone, Debug)]
    struct RuntimeModule {
        base: u64,
        image: PeImage,
        executable_sections: Vec<SectionBytes>,
        digest: [u8; 32],
    }

    pub(super) struct CaptureSource {
        pub(super) memory: File,
        pub(super) module_size: u32,
        pub(super) module_digest: [u8; 32],
        pub(super) build_info_size: u64,
        pub(super) build_info_digest: [u8; 32],
        pub(super) scan: ScanResult,
    }

    impl CaptureSource {
        pub(super) fn read_record(&self, target: u64) -> Option<[u8; HOVER_RECORD_SIZE]> {
            let mut record = [0; HOVER_RECORD_SIZE];
            let memory = FileMemory(self.memory.try_clone().ok()?);
            memory.read_once(target, &mut record).then_some(record)
        }
    }

    pub(super) fn discover(build_info: &Path) -> Result<CaptureSource, ProbeError> {
        let (build_info_size, build_info_digest) = hash_file(build_info)?;
        let entries = fs::read_dir(PROC_ROOT).map_err(|_| ProbeError::ProcessDiscovery)?;
        let mut pairs = Vec::new();

        for entry in entries {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            if !argv0_is_d2r(&cmdline) {
                continue;
            }
            let maps_text = fs::read_to_string(entry.path().join("maps"))
                .map_err(|_| ProbeError::ProcessDiscovery)?;
            let maps = parse_maps(&maps_text).ok_or(ProbeError::ProcessDiscovery)?;
            let memory = File::open(entry.path().join("mem"))
                .map(FileMemory)
                .map_err(|_| ProbeError::ProcessRead)?;
            let modules = discover_modules(&maps, &memory);
            for module in modules {
                pairs.push((entry.path(), module));
            }
        }

        if pairs.len() != 1 {
            return Err(ProbeError::ProcessDiscovery);
        }
        let (process_path, module) = pairs.pop().ok_or(ProbeError::ProcessDiscovery)?;
        let memory = File::open(process_path.join("mem")).map_err(|_| ProbeError::ProcessRead)?;
        let reader = FileMemory(memory.try_clone().map_err(|_| ProbeError::ProcessRead)?);
        let scan = scan_patterns(
            module.base,
            &module.executable_sections,
            |address, record| reader.read_once(address, record),
        );

        Ok(CaptureSource {
            memory,
            module_size: module.image.size_of_image,
            module_digest: module.digest,
            build_info_size,
            build_info_digest,
            scan,
        })
    }

    fn argv0_is_d2r(cmdline: &[u8]) -> bool {
        let argv0 = cmdline.split(|byte| *byte == 0).next().unwrap_or_default();
        let basename = argv0
            .rsplit(|byte| *byte == b'/' || *byte == b'\\')
            .next()
            .unwrap_or_default();
        basename.eq_ignore_ascii_case(b"D2R.exe")
    }

    fn parse_maps(text: &str) -> Option<Vec<MapEntry>> {
        text.lines().map(parse_map_line).collect()
    }

    fn parse_map_line(line: &str) -> Option<MapEntry> {
        let mut fields = line.split_whitespace();
        let (start, end) = fields.next()?.split_once('-')?;
        let permissions = fields.next()?;
        let offset = u64::from_str_radix(fields.next()?, 16).ok()?;
        fields.next()?;
        fields.next()?;
        let anonymous = fields.next().is_none();
        let start = u64::from_str_radix(start, 16).ok()?;
        let end = u64::from_str_radix(end, 16).ok()?;
        (start < end).then_some(MapEntry {
            range: ReadableRange { start, end },
            readable: permissions.as_bytes().first() == Some(&b'r'),
            offset,
            anonymous,
        })
    }

    fn discover_modules(maps: &[MapEntry], memory: &impl MemoryReader) -> Vec<RuntimeModule> {
        let readable: Vec<_> = maps
            .iter()
            .filter(|map| map.readable)
            .map(|map| map.range)
            .collect();
        maps.iter()
            .filter(|map| map.readable && (map.offset == 0 || map.anonymous))
            .filter_map(|map| read_runtime_module(map.range.start, &readable, memory))
            .collect()
    }

    fn read_runtime_module(
        base: u64,
        readable: &[ReadableRange],
        memory: &impl MemoryReader,
    ) -> Option<RuntimeModule> {
        let headers = read_pe_headers(base, memory)?;
        let image = parse_pe(&headers).ok()?;
        if image.sections.is_empty()
            || image.sections.iter().any(|section| {
                let Some(start) = base.checked_add(u64::from(section.rva)) else {
                    return true;
                };
                !range_is_covered(readable, start, u64::from(section.size))
            })
        {
            return None;
        }

        let mut executable_sections = Vec::new();
        for section in image.sections.iter().filter(|section| section.executable) {
            let start = base.checked_add(u64::from(section.rva))?;
            let size = usize::try_from(section.size).ok()?;
            let mut bytes = vec![0; size];
            if !memory.read_once(start, &mut bytes) {
                return None;
            }
            executable_sections.push(SectionBytes {
                name: section.name,
                rva: section.rva,
                declared_size: section.size,
                bytes,
            });
        }
        if executable_sections.is_empty() {
            return None;
        }

        let digest = hash_module(&image, &executable_sections);
        Some(RuntimeModule {
            base,
            image,
            executable_sections,
            digest,
        })
    }

    fn read_pe_headers(base: u64, memory: &impl MemoryReader) -> Option<Vec<u8>> {
        let mut dos = [0; 0x40];
        if !memory.read_once(base, &mut dos) || &dos[..2] != b"MZ" {
            return None;
        }
        let pe_offset = u64::from(read_u32(&dos, 0x3c).ok()?);
        let pe_address = base.checked_add(pe_offset)?;
        let mut fixed = [0; 24];
        if !memory.read_once(pe_address, &mut fixed) || &fixed[..4] != b"PE\0\0" {
            return None;
        }
        let sections = usize::from(read_u16(&fixed, 6).ok()?);
        let optional_size = usize::from(read_u16(&fixed, 20).ok()?);
        let header_len = usize::try_from(pe_offset)
            .ok()?
            .checked_add(24)?
            .checked_add(optional_size)?
            .checked_add(sections.checked_mul(40)?)?;
        let mut headers = vec![0; header_len];
        memory.read_once(base, &mut headers).then_some(headers)
    }

    fn hash_module(image: &PeImage, sections: &[SectionBytes]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(image.machine.to_le_bytes());
        hash.update(image.size_of_image.to_le_bytes());
        for section in sections {
            hash.update(section.name);
            hash.update(section.rva.to_le_bytes());
            hash.update(section.declared_size.to_le_bytes());
            hash.update(&section.bytes);
        }
        hash.finalize().into()
    }

    fn hash_file(path: &Path) -> Result<(u64, [u8; 32]), ProbeError> {
        let mut file = File::open(path).map_err(|_| ProbeError::BuildInfoRead)?;
        let mut hash = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0; 8192];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| ProbeError::BuildInfoRead)?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(read).map_err(|_| ProbeError::BuildInfoRead)?)
                .ok_or(ProbeError::BuildInfoRead)?;
            hash.update(&buffer[..read]);
        }
        Ok((total, hash.finalize().into()))
    }

    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;

        use super::*;
        use crate::{IMAGE_SCN_MEM_EXECUTE, tests::pe_fixture};

        struct FixtureMemory(BTreeMap<u64, Vec<u8>>);

        impl MemoryReader for FixtureMemory {
            fn read_once(&self, address: u64, destination: &mut [u8]) -> bool {
                self.0.iter().any(|(start, bytes)| {
                    let Some(offset) = address.checked_sub(*start) else {
                        return false;
                    };
                    let Ok(offset) = usize::try_from(offset) else {
                        return false;
                    };
                    let Some(source) = bytes.get(offset..offset.saturating_add(destination.len()))
                    else {
                        return false;
                    };
                    destination.copy_from_slice(source);
                    true
                })
            }
        }

        fn runtime_fixture(base: u64, section_size: u32) -> (Vec<MapEntry>, FixtureMemory) {
            let headers = pe_fixture(&[(b".text", 0x1000, section_size, IMAGE_SCN_MEM_EXECUTE)]);
            let mut image = vec![0; 0x1000 + section_size as usize];
            image[..headers.len()].copy_from_slice(&headers);
            let maps = vec![MapEntry {
                range: ReadableRange {
                    start: base,
                    end: base + image.len() as u64,
                },
                readable: true,
                offset: 0,
                anonymous: false,
            }];
            (maps, FixtureMemory(BTreeMap::from([(base, image)])))
        }

        #[test]
        fn process_name_uses_argv0_basename_only() {
            assert!(argv0_is_d2r(b"Z:\\games\\D2R.EXE\0-online\0"));
            assert!(argv0_is_d2r(b"/games/D2R.exe\0"));
            assert!(!argv0_is_d2r(b"/usr/bin/proton\0/games/D2R.exe\0"));
            assert!(!argv0_is_d2r(b"D2R.exe.wrapper\0"));
        }

        #[test]
        fn parses_named_and_anonymous_maps() {
            let maps = parse_maps(
                "1000-2000 r--p 00000000 00:00 0 /games/D2R.exe\n\
                 2000-3000 r-xp 00001000 00:00 0\n\
                 3000-4000 ---p 00000000 00:00 0 [guard]\n",
            )
            .unwrap();
            assert!(!maps[0].anonymous);
            assert!(maps[1].anonymous);
            assert!(!maps[2].readable);
            assert!(parse_maps("not-a-map").is_none());
        }

        #[test]
        fn discovers_named_or_anonymous_structural_pe() {
            let base = 0x1000_0000;
            let (maps, memory) = runtime_fixture(base, 0x40);
            assert_eq!(discover_modules(&maps, &memory).len(), 1);
            let mut anonymous_maps = maps.clone();
            anonymous_maps[0].anonymous = true;
            anonymous_maps[0].offset = 0x1000;
            assert_eq!(discover_modules(&anonymous_maps, &memory).len(), 1);
        }

        #[test]
        fn requires_covered_sections_and_exact_reads() {
            let base = 0x2000_0000;
            let (mut maps, memory) = runtime_fixture(base, 0x40);
            maps[0].range.end = base + 0x1020;
            assert!(discover_modules(&maps, &memory).is_empty());

            let (maps, mut memory) = runtime_fixture(base, 0x40);
            memory.0.get_mut(&base).unwrap().truncate(0x1020);
            assert!(discover_modules(&maps, &memory).is_empty());
        }

        #[test]
        fn rejects_absent_executable_sections_and_duplicate_modules() {
            let base = 0x3000_0000;
            let headers = pe_fixture(&[(b".data", 0x1000, 0x20, 0)]);
            let mut image = vec![0; 0x1020];
            image[..headers.len()].copy_from_slice(&headers);
            let memory = FixtureMemory(BTreeMap::from([(base, image)]));
            let one = MapEntry {
                range: ReadableRange {
                    start: base,
                    end: base + 0x1020,
                },
                readable: true,
                offset: 0,
                anonymous: false,
            };
            assert!(discover_modules(&[one], &memory).is_empty());

            let (mut maps, memory) = runtime_fixture(base, 0x20);
            let second_base = base + 0x20_000;
            let second_image = memory.0.get(&base).unwrap().clone();
            let mut storage = memory.0;
            storage.insert(second_base, second_image);
            maps.push(MapEntry {
                range: ReadableRange {
                    start: second_base,
                    end: second_base + 0x1020,
                },
                readable: true,
                offset: 0,
                anonymous: true,
            });
            assert_eq!(discover_modules(&maps, &FixtureMemory(storage)).len(), 2);
        }

        #[test]
        fn module_digest_is_deterministic_and_sensitive_to_runtime_bytes() {
            let base = 0x4000_0000;
            let (maps, mut memory) = runtime_fixture(base, 0x20);
            let first = discover_modules(&maps, &memory).pop().unwrap().digest;
            memory.0.get_mut(&base).unwrap()[0x1000] = 1;
            let second = discover_modules(&maps, &memory).pop().unwrap().digest;
            assert_ne!(first, second);
        }

        #[test]
        fn one_read_must_fill_the_destination() {
            let memory = FixtureMemory(BTreeMap::from([(100, vec![1, 2, 3])]));
            let mut complete = [0; 3];
            assert!(memory.read_once(100, &mut complete));
            let mut partial = [0; 4];
            assert!(!memory.read_once(100, &mut partial));
        }
    }
}

#[derive(Debug, Error)]
enum ProbeError {
    #[cfg(not(target_os = "linux"))]
    #[error("capture is unavailable on this platform")]
    UnsupportedPlatform,
    #[cfg(target_os = "linux")]
    #[error("process discovery failed")]
    ProcessDiscovery,
    #[cfg(any(target_os = "linux", test))]
    #[error("process memory read failed")]
    ProcessRead,
    #[cfg(target_os = "linux")]
    #[error("build metadata read failed")]
    BuildInfoRead,
    #[cfg(target_os = "linux")]
    #[error("operator confirmation failed")]
    OperatorInput,
    #[error("report output failed")]
    Output,
}

#[cfg(any(target_os = "linux", test))]
const PROMPTS: [&str; 6] = [
    "Hover the prepared loose inventory rune, then press Enter.",
    "Move the cursor away from every item, then press Enter.",
    "Hover the same loose inventory rune again, then press Enter.",
    "Hover the prepared fixed-name Unique item, then press Enter.",
    "Hover the prepared fixed-name Set item, then press Enter.",
    "Hover the separate same-kind rune in the stacked stash, then press Enter.",
];
#[cfg(any(target_os = "linux", test))]
const STAGES: [&str; 6] = [
    "loose_rune_first",
    "away",
    "loose_rune_repeat",
    "unique",
    "set",
    "stacked_rune",
];

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Record {
    active: u8,
    opaque_flag: u8,
    unit_type: u32,
    identity: u32,
}

#[cfg(any(target_os = "linux", test))]
fn decode_record(bytes: [u8; 12]) -> Record {
    Record {
        active: bytes[0],
        opaque_flag: bytes[1],
        unit_type: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        identity: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    }
}

#[cfg(any(target_os = "linux", test))]
fn traditional(records: &[Record]) -> bool {
    records.len() == 6
        && records.iter().all(|r| r.active <= 1)
        && records[0].active == 1
        && records[0].unit_type == 4
        && records[0].identity != 0
        && records[1].active == 0
        && records[2].active == 1
        && records[2].unit_type == 4
        && records[2].identity == records[0].identity
        && records[3].active == 1
        && records[3].unit_type == 4
        && records[3].identity != 0
        && records[3].identity != records[0].identity
        && records[4].active == 1
        && records[4].unit_type == 4
        && records[4].identity != 0
        && records[4].identity != records[0].identity
        && records[4].identity != records[3].identity
}

#[cfg(any(target_os = "linux", test))]
fn stacked_supported(records: &[Record]) -> bool {
    traditional(records)
        && records[5].active == 1
        && records[5].unit_type == 4
        && records[5].identity != 0
        && ![
            records[0].identity,
            records[3].identity,
            records[4].identity,
        ]
        .contains(&records[5].identity)
}

#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct Metadata {
    module_digest: String,
    module_size: u32,
    build_info_digest: String,
    build_info_size: u64,
}
#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct ScanReport {
    patterns: Vec<HitReport>,
    unit_hash_hits: usize,
    candidate_count: usize,
}
#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct HitReport {
    name: &'static str,
    hits: usize,
}
#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct Relations {
    nonzero: bool,
    same_as_loose_rune_first: bool,
    distinct_from_prior_items: bool,
}
#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct StageReport {
    stage: &'static str,
    active: u8,
    opaque_flag: u8,
    unit_type: u32,
    identity_relations: Relations,
}
#[cfg(any(target_os = "linux", test))]
#[derive(Serialize)]
struct CandidateReport {
    ordinal: usize,
    patterns: Vec<&'static str>,
    traditional_valid: bool,
    stages: Vec<StageReport>,
}
#[derive(Serialize)]
struct ErrorReport {
    stage: &'static str,
    category: &'static str,
}
#[derive(Serialize)]
struct Report {
    schema_version: u8,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorReport>,
    #[cfg(any(target_os = "linux", test))]
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
    #[cfg(any(target_os = "linux", test))]
    #[serde(skip_serializing_if = "Option::is_none")]
    scan: Option<ScanReport>,
    #[cfg(any(target_os = "linux", test))]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    candidates: Vec<CandidateReport>,
    #[cfg(any(target_os = "linux", test))]
    #[serde(skip_serializing_if = "Option::is_none")]
    stacked: Option<&'static str>,
}

fn report_error(error: &ProbeError) -> Report {
    let (stage, category) = match error {
        #[cfg(not(target_os = "linux"))]
        ProbeError::UnsupportedPlatform => ("platform", "unsupported"),
        #[cfg(target_os = "linux")]
        ProbeError::ProcessDiscovery => ("discovery", "selection"),
        #[cfg(any(target_os = "linux", test))]
        ProbeError::ProcessRead => ("capture", "read"),
        #[cfg(target_os = "linux")]
        ProbeError::BuildInfoRead => ("build_info", "read"),
        #[cfg(target_os = "linux")]
        ProbeError::OperatorInput => ("confirmation", "input"),
        ProbeError::Output => ("report", "output"),
    };
    Report {
        schema_version: 1,
        status: "error",
        error: Some(ErrorReport { stage, category }),
        #[cfg(any(target_os = "linux", test))]
        metadata: None,
        #[cfg(any(target_os = "linux", test))]
        scan: None,
        #[cfg(any(target_os = "linux", test))]
        candidates: vec![],
        #[cfg(any(target_os = "linux", test))]
        stacked: None,
    }
}

#[cfg(any(target_os = "linux", test))]
fn candidate_reports(scan: &ScanResult, snapshots: &[Vec<Record>]) -> Vec<CandidateReport> {
    scan.candidates
        .iter()
        .zip(snapshots)
        .enumerate()
        .map(|(index, (candidate, records))| {
            let stages = records
                .iter()
                .enumerate()
                .map(|(stage, record)| {
                    let prior = [0_usize, 3, 4]
                        .into_iter()
                        .filter(|prior| *prior < stage)
                        .map(|prior| records[prior].identity)
                        .collect::<Vec<_>>();
                    StageReport {
                        stage: STAGES[stage],
                        active: record.active,
                        opaque_flag: record.opaque_flag,
                        unit_type: record.unit_type,
                        identity_relations: Relations {
                            nonzero: record.identity != 0,
                            same_as_loose_rune_first: record.identity != 0
                                && record.identity == records[0].identity,
                            distinct_from_prior_items: record.identity != 0
                                && prior.iter().all(|identity| *identity != record.identity),
                        },
                    }
                })
                .collect();
            CandidateReport {
                ordinal: index + 1,
                patterns: candidate.pattern_names.clone(),
                traditional_valid: traditional(records),
                stages,
            }
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn capture_sequence(
    targets: &[u64],
    mut confirm: impl FnMut(&str) -> Result<(), ProbeError>,
    mut read: impl FnMut(u64) -> Option<[u8; 12]>,
) -> Result<Vec<Vec<Record>>, ProbeError> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let mut snapshots = vec![Vec::with_capacity(6); targets.len()];
    for prompt in PROMPTS {
        confirm(prompt)?;
        for (index, target) in targets.iter().enumerate() {
            snapshots[index].push(decode_record(read(*target).ok_or(ProbeError::ProcessRead)?));
        }
    }
    Ok(snapshots)
}

#[cfg(target_os = "linux")]
fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(report: &Report, output: &mut impl Write) -> Result<(), ProbeError> {
    serde_json::to_writer(&mut *output, report).map_err(|_| ProbeError::Output)?;
    output.write_all(b"\n").map_err(|_| ProbeError::Output)
}

pub fn run_capture(
    build_info: &Path,
    input: &mut impl BufRead,
    prompts: &mut impl Write,
    output: &mut impl Write,
) -> u8 {
    match capture_inner(build_info, input, prompts) {
        Ok((report, code)) => {
            if write_report(&report, output).is_ok() {
                code
            } else {
                3
            }
        }
        Err(error) => {
            let code = match error {
                #[cfg(not(target_os = "linux"))]
                ProbeError::UnsupportedPlatform => 2,
                #[cfg(target_os = "linux")]
                ProbeError::OperatorInput => 2,
                _ => 3,
            };
            let _ = write_report(&report_error(&error), output);
            code
        }
    }
}

#[cfg(target_os = "linux")]
fn capture_inner(
    build_info: &Path,
    input: &mut impl BufRead,
    prompts: &mut impl Write,
) -> Result<(Report, u8), ProbeError> {
    let source = linux::discover(build_info)?;
    let targets: Vec<_> = source
        .scan
        .candidates
        .iter()
        .map(|candidate| candidate.target)
        .collect();
    let snapshots = capture_sequence(
        &targets,
        |prompt| {
            writeln!(prompts, "{prompt}")
                .and_then(|_| prompts.flush())
                .map_err(|_| ProbeError::OperatorInput)?;
            let mut line = String::new();
            if input
                .read_line(&mut line)
                .map_err(|_| ProbeError::OperatorInput)?
                == 0
                || !line.trim_end_matches(['\r', '\n']).is_empty()
            {
                return Err(ProbeError::OperatorInput);
            }
            Ok(())
        },
        |target| source.read_record(target),
    )?;
    let candidates = candidate_reports(&source.scan, &snapshots);
    let passing: Vec<_> = snapshots
        .iter()
        .enumerate()
        .filter(|(_, records)| traditional(records))
        .map(|(index, _)| index)
        .collect();
    let (status, code, stacked) = match passing.as_slice() {
        [] => ("no_valid_candidate", 4, None),
        [index] => (
            "success",
            0,
            Some(if stacked_supported(&snapshots[*index]) {
                "supported"
            } else {
                "unresolved"
            }),
        ),
        _ => ("ambiguous", 5, None),
    };
    let report = Report {
        schema_version: 1,
        status,
        error: None,
        metadata: Some(Metadata {
            module_digest: digest_hex(source.module_digest),
            module_size: source.module_size,
            build_info_digest: digest_hex(source.build_info_digest),
            build_info_size: source.build_info_size,
        }),
        scan: Some(ScanReport {
            patterns: source
                .scan
                .patterns
                .iter()
                .map(|pattern| HitReport {
                    name: pattern.name,
                    hits: pattern.hits,
                })
                .collect(),
            unit_hash_hits: source.scan.unit_hash_hits,
            candidate_count: source.scan.candidates.len(),
        }),
        candidates,
        stacked,
    };
    Ok((report, code))
}

#[cfg(not(target_os = "linux"))]
fn capture_inner(
    _build_info: &Path,
    _input: &mut impl BufRead,
    _prompts: &mut impl Write,
) -> Result<(Report, u8), ProbeError> {
    Err(ProbeError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x1000_0000;

    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn pe_fixture(sections: &[(&[u8], u32, u32, u32)]) -> Vec<u8> {
        let pe_offset = 0x80;
        let optional = pe_offset + 24;
        let section_table = optional + 0xf0;
        let mut bytes = vec![0; section_table + sections.len() * 40];
        bytes[0..2].copy_from_slice(b"MZ");
        put_u32(&mut bytes, 0x3c, pe_offset as u32);
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        put_u16(&mut bytes, pe_offset + 4, X86_64_MACHINE);
        put_u16(&mut bytes, pe_offset + 6, sections.len() as u16);
        put_u16(&mut bytes, pe_offset + 20, 0xf0);
        put_u16(&mut bytes, optional, PE32_PLUS_MAGIC);
        put_u32(&mut bytes, optional + 56, 0x8000);
        for (index, (name, rva, size, flags)) in sections.iter().enumerate() {
            let row = section_table + index * 40;
            bytes[row..row + name.len()].copy_from_slice(name);
            put_u32(&mut bytes, row + 8, *size);
            put_u32(&mut bytes, row + 12, *rva);
            put_u32(&mut bytes, row + 36, *flags);
        }
        bytes
    }

    fn executable_section(rva: u32, bytes: Vec<u8>) -> SectionBytes {
        SectionBytes {
            name: *b".text\0\0\0",
            rva,
            declared_size: bytes.len() as u32,
            bytes,
        }
    }

    fn materialize(pattern: &[Option<u8>], displacement: i32) -> Vec<u8> {
        let mut bytes: Vec<u8> = pattern.iter().map(|byte| byte.unwrap_or(0xa5)).collect();
        bytes[3..7].copy_from_slice(&displacement.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_pe32_plus_sections() {
        let fixture = pe_fixture(&[
            (b".text", 0x1000, 0x120, IMAGE_SCN_MEM_EXECUTE),
            (b".data", 0x3000, 0x80, 0),
        ]);
        let image = parse_pe(&fixture).unwrap();
        assert_eq!(image.machine, X86_64_MACHINE);
        assert_eq!(image.size_of_image, 0x8000);
        assert_eq!(image.sections.len(), 2);
        assert!(image.sections[0].executable);
        assert!(!image.sections[1].executable);
    }

    #[test]
    fn rejects_invalid_and_truncated_pe_headers() {
        let valid = pe_fixture(&[(b".text", 0x1000, 0x20, IMAGE_SCN_MEM_EXECUTE)]);
        let mut cases = Vec::new();
        let mut bad_dos = valid.clone();
        bad_dos[0] = 0;
        cases.push((bad_dos, PeError::Dos));
        let mut bad_offset = valid.clone();
        put_u32(&mut bad_offset, 0x3c, 0xffff_ffff);
        cases.push((bad_offset, PeError::Truncated));
        let mut bad_signature = valid.clone();
        bad_signature[0x80] = 0;
        cases.push((bad_signature, PeError::Signature));
        let mut bad_machine = valid.clone();
        put_u16(&mut bad_machine, 0x84, 0x014c);
        cases.push((bad_machine, PeError::Unsupported));
        let mut bad_magic = valid.clone();
        put_u16(&mut bad_magic, 0x98, 0x010b);
        cases.push((bad_magic, PeError::Unsupported));
        let mut dll = valid.clone();
        put_u16(&mut dll, 0x96, IMAGE_FILE_DLL);
        cases.push((dll, PeError::Dll));
        cases.push((valid[..valid.len() - 1].to_vec(), PeError::Truncated));
        for (bytes, expected) in cases {
            assert_eq!(parse_pe(&bytes), Err(expected));
        }
    }

    #[test]
    fn rejects_invalid_section_ranges_and_arithmetic() {
        let fixture = pe_fixture(&[(b".text", 0x7ff0, 0x20, IMAGE_SCN_MEM_EXECUTE)]);
        assert_eq!(parse_pe(&fixture), Err(PeError::SectionRange));
        assert_eq!(read_bytes(&[], usize::MAX, 2), Err(PeError::Arithmetic));
    }

    #[test]
    fn readable_range_union_covers_adjacent_mappings_only() {
        let adjacent = [
            ReadableRange {
                start: 100,
                end: 120,
            },
            ReadableRange {
                start: 120,
                end: 150,
            },
        ];
        assert!(range_is_covered(&adjacent, 110, 35));
        let gap = [
            adjacent[0],
            ReadableRange {
                start: 121,
                end: 150,
            },
        ];
        assert!(!range_is_covered(&gap, 110, 35));
        assert!(!range_is_covered(&adjacent, u64::MAX - 1, 4));
    }

    #[test]
    fn scans_all_patterns_and_deduplicates_nested_mouse_signatures() {
        let direct_target = BASE - 0x20;
        let direct_displacement = i32::try_from(direct_target as i128 - BASE as i128 + 1).unwrap();
        let mouse_rva = 0x1100_u32;
        let mouse_target = BASE + 0x5000;
        let mouse_displacement =
            i32::try_from(mouse_target as i128 - (BASE + u64::from(mouse_rva) + 7) as i128)
                .unwrap();

        let mut first = vec![0x90; 64];
        first[4..4 + LAST_HOVER_V3.len()]
            .copy_from_slice(&materialize(LAST_HOVER_V3, direct_displacement));
        let mut second = vec![0x90; 80];
        second[..MOUSE_V1.len()].copy_from_slice(&materialize(MOUSE_V1, mouse_displacement));
        second[40..40 + UNIT_HASH_PATTERN.len()].copy_from_slice(UNIT_HASH_PATTERN);
        let sections = [
            executable_section(0x1000, first),
            executable_section(mouse_rva, second),
        ];

        let result = scan_patterns(BASE, &sections, |_, record| {
            record.fill(0);
            true
        });
        assert_eq!(
            result.patterns,
            vec![
                PatternCount {
                    name: "last-hover-v3",
                    hits: 1
                },
                PatternCount {
                    name: "mouse-v1",
                    hits: 1
                },
                PatternCount {
                    name: "mouse-short",
                    hits: 1
                },
                PatternCount {
                    name: "mouse-lea",
                    hits: 1
                },
            ]
        );
        assert_eq!(result.unit_hash_hits, 1);
        assert_eq!(result.candidates.len(), 2);
        assert_eq!(result.candidates[0].target, direct_target);
        assert_eq!(result.candidates[0].pattern_names, vec!["last-hover-v3"]);
        assert_eq!(result.candidates[1].target, mouse_target);
        assert_eq!(
            result.candidates[1].pattern_names,
            vec!["mouse-v1", "mouse-short", "mouse-lea"]
        );
    }

    #[test]
    fn preserves_every_hit_and_rejects_unreadable_targets() {
        let rva = 0x2000_u32;
        let target = BASE + 0x4000;
        let displacement =
            i32::try_from(target as i128 - (BASE + u64::from(rva) + 7) as i128).unwrap();
        let signature = materialize(patterns()[3].bytes, displacement);
        let mut bytes = vec![0x90; signature.len() * 2 + 1];
        bytes[..signature.len()].copy_from_slice(&signature);
        bytes[signature.len() + 1..].copy_from_slice(&signature);
        let section = executable_section(rva, bytes);
        let result = scan_patterns(BASE, &[section], |_, _| false);
        assert_eq!(result.patterns[3].hits, 2);
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn patterns_do_not_cross_section_boundaries() {
        let signature = materialize(LAST_HOVER_V3, 1);
        let split = signature.len() / 2;
        let sections = [
            executable_section(0x1000, signature[..split].to_vec()),
            executable_section(0x2000, signature[split..].to_vec()),
        ];
        let result = scan_patterns(BASE, &sections, |_, _| true);
        assert!(result.patterns.iter().all(|pattern| pattern.hits == 0));
    }

    #[test]
    fn signed_resolution_fails_closed_on_overflow() {
        let direct = materialize(LAST_HOVER_V3, -0x20);
        assert_eq!(
            resolve_target(0x100, 0, &direct, Resolver::DirectDisplacement),
            Some(0xdf)
        );
        let rip = materialize(patterns()[3].bytes, -0x20);
        assert_eq!(
            resolve_target(0x100, 0x10, &rip, Resolver::RipRelative),
            Some(0xf7)
        );
        assert_eq!(
            resolve_target(0, 0, &direct, Resolver::DirectDisplacement),
            None
        );
        let positive = materialize(patterns()[3].bytes, i32::MAX);
        assert_eq!(
            resolve_target(u64::MAX, 1, &positive, Resolver::RipRelative),
            None
        );
    }

    fn record(active: u8, unit_type: u32, identity: u32) -> Record {
        Record {
            active,
            opaque_flag: 77,
            unit_type,
            identity,
        }
    }

    fn passing_records(stacked_identity: u32) -> Vec<Record> {
        vec![
            record(1, 4, 10),
            record(0, 999, 999),
            record(1, 4, 10),
            record(1, 4, 20),
            record(1, 4, 30),
            record(1, 4, stacked_identity),
        ]
    }

    #[test]
    fn classifier_requires_the_complete_traditional_sequence() {
        let valid = passing_records(10);
        assert!(traditional(&valid));
        assert!(!stacked_supported(&valid));
        let supported = passing_records(40);
        assert!(traditional(&supported));
        assert!(stacked_supported(&supported));
        for changed in [
            (0, record(1, 3, 10)),
            (0, record(1, 4, 0)),
            (1, record(1, 4, 0)),
            (2, record(1, 4, 11)),
            (3, record(1, 4, 10)),
            (4, record(1, 4, 20)),
            (5, record(2, 4, 40)),
        ] {
            let mut invalid = supported.clone();
            invalid[changed.0] = changed.1;
            assert!(!traditional(&invalid));
        }
        let mut opaque_changed = supported;
        opaque_changed
            .iter_mut()
            .for_each(|item| item.opaque_flag = 255);
        assert!(traditional(&opaque_changed));
    }

    #[test]
    fn zero_one_and_multiple_passes_remain_distinct() {
        let passing = passing_records(40);
        let mut failing = passing.clone();
        failing[1].active = 1;
        assert_eq!(
            [&failing]
                .into_iter()
                .filter(|records| traditional(records))
                .count(),
            0
        );
        assert_eq!(
            [&passing]
                .into_iter()
                .filter(|records| traditional(records))
                .count(),
            1
        );
        assert_eq!(
            [&passing, &passing]
                .into_iter()
                .filter(|records| traditional(records))
                .count(),
            2
        );
    }

    #[test]
    fn sequence_prompts_once_per_stage_reads_once_per_candidate_and_fails_closed() {
        let mut empty_prompts = 0;
        let mut empty_reads = 0;
        let empty = capture_sequence(
            &[],
            |_| {
                empty_prompts += 1;
                Ok(())
            },
            |_| {
                empty_reads += 1;
                None
            },
        )
        .unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty_prompts, 0);
        assert_eq!(empty_reads, 0);

        let mut prompts = Vec::new();
        let mut reads = 0;
        let snapshots = capture_sequence(
            &[10, 20],
            |prompt| {
                prompts.push(prompt.to_owned());
                Ok(())
            },
            |_| {
                reads += 1;
                Some([0; 12])
            },
        )
        .unwrap();
        assert_eq!(prompts, PROMPTS);
        assert_eq!(
            prompts,
            vec![
                "Hover the prepared loose inventory rune, then press Enter.",
                "Move the cursor away from every item, then press Enter.",
                "Hover the same loose inventory rune again, then press Enter.",
                "Hover the prepared fixed-name Unique item, then press Enter.",
                "Hover the prepared fixed-name Set item, then press Enter.",
                "Hover the separate same-kind rune in the stacked stash, then press Enter.",
            ]
        );
        assert!(
            prompts
                .iter()
                .all(|prompt| { !prompt.to_ascii_lowercase().contains(concat!("ve", "x")) })
        );
        assert_eq!(reads, 12);
        assert_eq!(
            snapshots.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![6, 6]
        );
        assert!(matches!(
            capture_sequence(&[10], |_| Ok(()), |_| None),
            Err(ProbeError::ProcessRead)
        ));
    }

    #[test]
    fn stage_names_are_generic() {
        assert_eq!(
            STAGES,
            [
                "loose_rune_first",
                "away",
                "loose_rune_repeat",
                "unique",
                "set",
                "stacked_rune"
            ]
        );
    }

    #[test]
    fn serialized_report_recursively_excludes_runtime_identifiers() {
        let scan = ScanResult {
            patterns: vec![PatternCount {
                name: "mouse-lea",
                hits: 1,
            }],
            unit_hash_hits: 0,
            candidates: vec![Candidate {
                target: 0x1234_5678,
                pattern_names: vec!["mouse-lea"],
            }],
        };
        let report = Report {
            schema_version: 1,
            status: "success",
            error: None,
            metadata: Some(Metadata {
                module_digest: "a".repeat(64),
                module_size: 22_380_544,
                build_info_digest: "b".repeat(64),
                build_info_size: 100,
            }),
            scan: Some(ScanReport {
                patterns: vec![HitReport {
                    name: "mouse-lea",
                    hits: 1,
                }],
                unit_hash_hits: 0,
                candidate_count: 1,
            }),
            candidates: candidate_reports(&scan, &[passing_records(40)]),
            stacked: Some("supported"),
        };
        let value = serde_json::to_value(report).unwrap();
        let text = serde_json::to_string(&value).unwrap();
        assert!(!text.to_ascii_lowercase().contains(concat!("ve", "x")));
        assert!(text.contains("\"same_as_loose_rune_first\""));
        assert!(text.contains("\"loose_rune_first\""));
        assert!(text.contains("\"stacked_rune\""));
        fn inspect(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, value) in object {
                        assert!(
                            ![
                                "pid",
                                "address",
                                "path",
                                "command_line",
                                "memory_bytes",
                                "unit_id",
                                "raw_id"
                            ]
                            .contains(&key.as_str())
                        );
                        inspect(value);
                    }
                }
                serde_json::Value::Array(values) => values.iter().for_each(inspect),
                serde_json::Value::String(text) => {
                    assert!(!text.starts_with("0x"));
                    assert!(!text.contains('/'));
                    assert!(!text.contains('\\'));
                }
                _ => {}
            }
        }
        inspect(&value);
        assert!(!text.contains("305419896"));
    }
}
