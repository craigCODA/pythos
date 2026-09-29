pub const MAX_CAPABILITY_ENTRIES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityKind {
    PowerManagement,
    Pcie,
    Msi,
    Msix,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciCapabilityEntry {
    pub id: u8,
    pub offset: u8,
    pub next: u8,
    pub kind: CapabilityKind,
    pub header_len: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub entries: [Option<PciCapabilityEntry>; MAX_CAPABILITY_ENTRIES],
    pub entry_count: usize,
    pub first_pointer: u8,
    pub power_management_offset: Option<u8>,
    pub pcie_offset: Option<u8>,
    pub msi_offset: Option<u8>,
    pub msix_offset: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityParseError {
    InvalidPointer { offset: u8 },
    RepeatedOffset { offset: u8 },
    EntryLimitExceeded,
    RecognizedHeaderOutOfBounds { id: u8, offset: u8, header_len: u8 },
}

pub fn parse_capability_list<R: FnMut(u8) -> u8>(
    status: u16,
    first_pointer: u8,
    read_byte: &mut R,
) -> Result<CapabilitySnapshot, CapabilityParseError> {
    let mut snapshot = CapabilitySnapshot {
        entries: [None; MAX_CAPABILITY_ENTRIES],
        entry_count: 0,
        first_pointer: 0,
        power_management_offset: None,
        pcie_offset: None,
        msi_offset: None,
        msix_offset: None,
    };

    if status & (1 << 4) == 0 {
        return Ok(snapshot);
    }

    snapshot.first_pointer = first_pointer;
    let mut current = first_pointer;
    let mut visited: [u8; MAX_CAPABILITY_ENTRIES] = [0; MAX_CAPABILITY_ENTRIES];

    while current != 0 {
        if snapshot.entry_count == MAX_CAPABILITY_ENTRIES {
            return Err(CapabilityParseError::EntryLimitExceeded);
        }
        validate_pointer(current)?;

        for offset in visited.iter().take(snapshot.entry_count) {
            if *offset == current {
                return Err(CapabilityParseError::RepeatedOffset { offset: current });
            }
        }
        visited[snapshot.entry_count] = current;

        let id = read_byte(current);
        let next = read_byte(current + 1);
        let (kind, header_len) = classify_capability(id);
        if let Some(length) = header_len {
            if u16::from(current) + u16::from(length) - 1 > u16::from(u8::MAX) {
                return Err(CapabilityParseError::RecognizedHeaderOutOfBounds {
                    id,
                    offset: current,
                    header_len: length,
                });
            }
        }

        snapshot.entries[snapshot.entry_count] = Some(PciCapabilityEntry {
            id,
            offset: current,
            next,
            kind,
            header_len,
        });
        snapshot.entry_count += 1;
        record_first_offset(&mut snapshot, id, current);
        current = next;
    }

    Ok(snapshot)
}

fn validate_pointer(offset: u8) -> Result<(), CapabilityParseError> {
    // Conventional capability pointers are 4-byte aligned.
    if !(0x40..=0xFC).contains(&offset) || offset & 0x03 != 0 {
        return Err(CapabilityParseError::InvalidPointer { offset });
    }
    Ok(())
}

fn classify_capability(id: u8) -> (CapabilityKind, Option<u8>) {
    match id {
        0x01 => (CapabilityKind::PowerManagement, Some(0x08)),
        0x10 => (CapabilityKind::Pcie, Some(0x14)),
        0x05 => (CapabilityKind::Msi, Some(0x0A)),
        0x11 => (CapabilityKind::Msix, Some(0x0C)),
        _ => (CapabilityKind::Unknown, None),
    }
}

fn record_first_offset(snapshot: &mut CapabilitySnapshot, id: u8, offset: u8) {
    let target = match id {
        0x01 => &mut snapshot.power_management_offset,
        0x10 => &mut snapshot.pcie_offset,
        0x05 => &mut snapshot.msi_offset,
        0x11 => &mut snapshot.msix_offset,
        _ => return,
    };
    if target.is_none() {
        *target = Some(offset);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityKind, CapabilityParseError, MAX_CAPABILITY_ENTRIES, parse_capability_list,
    };

    fn parse(status: u16, first: u8, bytes: [u8; 256]) -> super::CapabilitySnapshot {
        parse_capability_list(status, first, &mut |offset| bytes[offset as usize]).unwrap()
    }

    #[test]
    fn absent_status_returns_deterministic_empty_snapshot_without_reading() {
        let mut reads = 0;
        let snapshot = parse_capability_list(0, 0x41, &mut |_| {
            reads += 1;
            0
        })
        .unwrap();

        assert_eq!(reads, 0);
        assert_eq!(snapshot.entries, [None; MAX_CAPABILITY_ENTRIES]);
        assert_eq!(snapshot.entry_count, 0);
        assert_eq!(snapshot.first_pointer, 0);
        assert_eq!(snapshot.power_management_offset, None);
        assert_eq!(snapshot.pcie_offset, None);
        assert_eq!(snapshot.msi_offset, None);
        assert_eq!(snapshot.msix_offset, None);
    }

    #[test]
    fn parses_recognized_entries_and_records_first_offsets() {
        let mut bytes = [0; 256];
        bytes[0x40] = 0x01;
        bytes[0x41] = 0x48;
        bytes[0x48] = 0x10;
        bytes[0x49] = 0x50;
        bytes[0x50] = 0x05;
        let snapshot = parse(1 << 4, 0x40, bytes);

        assert_eq!(snapshot.entry_count, 3);
        assert_eq!(
            snapshot.entries[0].unwrap().kind,
            CapabilityKind::PowerManagement
        );
        assert_eq!(snapshot.entries[0].unwrap().header_len, Some(0x08));
        assert_eq!(snapshot.entries[1].unwrap().kind, CapabilityKind::Pcie);
        assert_eq!(snapshot.entries[2].unwrap().kind, CapabilityKind::Msi);
        assert_eq!(snapshot.power_management_offset, Some(0x40));
        assert_eq!(snapshot.pcie_offset, Some(0x48));
        assert_eq!(snapshot.msi_offset, Some(0x50));
    }

    #[test]
    fn preserves_unknown_entries_without_a_semantic_length() {
        let mut bytes = [0; 256];
        bytes[0x40] = 0x7F;
        bytes[0x41] = 0x44;
        let entry = parse(1 << 4, 0x40, bytes).entries[0].unwrap();

        assert_eq!(entry.id, 0x7F);
        assert_eq!(entry.offset, 0x40);
        assert_eq!(entry.next, 0x44);
        assert_eq!(entry.kind, CapabilityKind::Unknown);
        assert_eq!(entry.header_len, None);
    }

    #[test]
    fn rejects_unaligned_and_out_of_range_initial_pointers() {
        for pointer in [0x41, 0x3C, 0x00] {
            if pointer == 0 {
                continue;
            }
            let result = parse_capability_list(1 << 4, pointer, &mut |_| 0);
            assert_eq!(
                result,
                Err(CapabilityParseError::InvalidPointer { offset: pointer })
            );
        }
    }

    #[test]
    fn rejects_invalid_next_pointer_before_reading_the_next_entry() {
        let mut bytes = [0; 256];
        bytes[0x40] = 0x7F;
        bytes[0x41] = 0x42;
        let result = parse_capability_list(1 << 4, 0x40, &mut |offset| bytes[offset as usize]);

        assert_eq!(
            result,
            Err(CapabilityParseError::InvalidPointer { offset: 0x42 })
        );
    }

    #[test]
    fn rejects_repeated_offsets() {
        let mut bytes = [0; 256];
        bytes[0x40] = 0x7F;
        bytes[0x41] = 0x44;
        bytes[0x44] = 0x7E;
        bytes[0x45] = 0x40;
        let result = parse_capability_list(1 << 4, 0x40, &mut |offset| bytes[offset as usize]);

        assert_eq!(
            result,
            Err(CapabilityParseError::RepeatedOffset { offset: 0x40 })
        );
    }

    #[test]
    fn accepts_exactly_48_entries_but_rejects_the_49th() {
        let mut bytes = [0; 256];
        for index in 0..MAX_CAPABILITY_ENTRIES {
            let offset = 0x40 + (index as u8) * 4;
            bytes[offset as usize] = 0x7F;
            bytes[offset as usize + 1] = if index + 1 == MAX_CAPABILITY_ENTRIES {
                0
            } else {
                offset + 4
            };
        }
        let snapshot = parse(1 << 4, 0x40, bytes);
        assert_eq!(snapshot.entry_count, MAX_CAPABILITY_ENTRIES);

        bytes[0xFC] = 0x7F;
        bytes[0xFD] = 0x40;
        let result = parse_capability_list(1 << 4, 0x40, &mut |offset| bytes[offset as usize]);
        assert_eq!(result, Err(CapabilityParseError::EntryLimitExceeded));
    }

    #[test]
    fn rejects_recognized_header_crossing_configuration_boundary() {
        let mut bytes = [0; 256];
        bytes[0xFC] = 0x10;
        let result = parse_capability_list(1 << 4, 0xFC, &mut |offset| bytes[offset as usize]);

        assert_eq!(
            result,
            Err(CapabilityParseError::RecognizedHeaderOutOfBounds {
                id: 0x10,
                offset: 0xFC,
                header_len: 0x14,
            })
        );
    }
}
