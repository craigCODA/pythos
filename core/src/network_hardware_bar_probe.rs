#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PciBarKind {
    Io,
    Memory32,
    MemoryBelow1MiB,
    Memory64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciBarSnapshot {
    pub raw_low: u32,
    pub raw_high: Option<u32>,
    pub kind: PciBarKind,
    pub base: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkBarLayout {
    pub raw: [u32; 6],
    pub bars: [Option<PciBarSnapshot>; 6],
    pub malformed: bool,
}

pub fn decode_bar_layout(raw: [u32; 6]) -> NetworkBarLayout {
    let mut bars = [None; 6];
    let mut malformed = false;
    let mut slot = 0;

    while slot < raw.len() {
        let low = raw[slot];
        if low == 0 {
            slot += 1;
            continue;
        }

        if low & 1 != 0 {
            bars[slot] = Some(PciBarSnapshot {
                raw_low: low,
                raw_high: None,
                kind: PciBarKind::Io,
                base: (low & !0b11) as u64,
            });
            slot += 1;
            continue;
        }

        match (low >> 1) & 0b11 {
            0 => {
                bars[slot] = Some(PciBarSnapshot {
                    raw_low: low,
                    raw_high: None,
                    kind: PciBarKind::Memory32,
                    base: (low & !0b1111) as u64,
                });
                slot += 1;
            }
            1 => {
                bars[slot] = Some(PciBarSnapshot {
                    raw_low: low,
                    raw_high: None,
                    kind: PciBarKind::MemoryBelow1MiB,
                    base: (low & !0b1111) as u64,
                });
                slot += 1;
            }
            2 => {
                if slot == raw.len() - 1 {
                    malformed = true;
                    slot += 1;
                    continue;
                }

                let high = raw[slot + 1];
                bars[slot] = Some(PciBarSnapshot {
                    raw_low: low,
                    raw_high: Some(high),
                    kind: PciBarKind::Memory64,
                    base: ((high as u64) << 32) | (low & !0b1111) as u64,
                });
                slot += 2;
            }
            _ => {
                malformed = true;
                slot += 1;
            }
        }
    }

    NetworkBarLayout {
        raw,
        bars,
        malformed,
    }
}

#[cfg(test)]
mod tests {
    use super::{PciBarKind, decode_bar_layout};

    #[test]
    fn preserves_all_raw_bar_dwords() {
        let raw = [0x0000_1001, 0xFEBF_0000, 0x0008_0002, 4, 5, 6];

        assert_eq!(decode_bar_layout(raw).raw, raw);
    }

    #[test]
    fn decodes_zero_as_unimplemented() {
        assert_eq!(decode_bar_layout([0; 6]).bars[0], None);
    }

    #[test]
    fn decodes_io_bar() {
        assert_eq!(
            decode_bar_layout([0x0000_1001, 0, 0, 0, 0, 0]).bars[0]
                .unwrap()
                .kind,
            PciBarKind::Io
        );
    }

    #[test]
    fn decodes_32_bit_memory_bar() {
        assert_eq!(
            decode_bar_layout([0xFEBF_0000, 0, 0, 0, 0, 0]).bars[0]
                .unwrap()
                .kind,
            PciBarKind::Memory32
        );
    }

    #[test]
    fn decodes_below_1mib_memory_bar() {
        let bar = decode_bar_layout([0x0008_0002, 0, 0, 0, 0, 0]).bars[0].unwrap();

        assert_eq!(bar.kind, PciBarKind::MemoryBelow1MiB);
        assert_eq!(bar.base, 0x0008_0000);
    }

    #[test]
    fn decodes_64_bit_memory_pair_and_consumes_high_slot() {
        let layout = decode_bar_layout([0x0000_0004, 0x0000_0001, 0, 0, 0, 0]);

        assert_eq!(layout.bars[0].unwrap().kind, PciBarKind::Memory64);
        assert_eq!(layout.bars[0].unwrap().base, 0x0000_0001_0000_0000);
        assert_eq!(layout.bars[1], None);
    }

    #[test]
    fn marks_64_bit_bar_in_final_slot_malformed() {
        assert!(decode_bar_layout([0, 0, 0, 0, 0, 0x0000_0004]).malformed);
    }

    #[test]
    fn marks_reserved_memory_type_malformed() {
        assert!(decode_bar_layout([0x0000_0006, 0, 0, 0, 0, 0]).malformed);
    }
}
