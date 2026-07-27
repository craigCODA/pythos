#![cfg_attr(test, allow(dead_code))]

const MAX_USER_COPY_MAPPINGS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserCopyAccess {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserCopyError {
    NullPointer,
    EmptyRange,
    LengthOverflow,
    OutOfRange,
    CrossMapping,
    PermissionDenied,
    TooManyMappings,
    OverlappingMapping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMapping {
    start: u64,
    end: u64,
    readable: bool,
    writable: bool,
}

impl UserMapping {
    pub fn new(
        start: u64,
        length: u64,
        readable: bool,
        writable: bool,
    ) -> Result<Self, UserCopyError> {
        if start == 0 {
            return Err(UserCopyError::NullPointer);
        }
        if length == 0 {
            return Err(UserCopyError::EmptyRange);
        }
        let end = start
            .checked_add(length)
            .ok_or(UserCopyError::LengthOverflow)?;
        Ok(Self {
            start,
            end,
            readable,
            writable,
        })
    }

    const fn contains_start(&self, ptr: u64) -> bool {
        ptr >= self.start && ptr < self.end
    }

    const fn contains_range(&self, ptr: u64, end: u64) -> bool {
        ptr >= self.start && end <= self.end
    }

    const fn overlaps_range(&self, ptr: u64, end: u64) -> bool {
        ptr < self.end && self.start < end
    }

    const fn allows(&self, access: UserCopyAccess) -> bool {
        match access {
            UserCopyAccess::Read => self.readable,
            UserCopyAccess::Write => self.writable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedUserRange {
    start: u64,
    end: u64,
    access: UserCopyAccess,
}

impl ValidatedUserRange {
    const fn new(start: u64, end: u64, access: UserCopyAccess) -> Self {
        Self { start, end, access }
    }

    pub const fn start(&self) -> u64 {
        self.start
    }

    pub const fn end(&self) -> u64 {
        self.end
    }

    pub const fn length(&self) -> u64 {
        self.end - self.start
    }

    pub const fn access(&self) -> UserCopyAccess {
        self.access
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserCopyMap {
    mappings: [Option<UserMapping>; MAX_USER_COPY_MAPPINGS],
    mapping_count: usize,
}

impl UserCopyMap {
    pub const fn new() -> Self {
        Self {
            mappings: [None; MAX_USER_COPY_MAPPINGS],
            mapping_count: 0,
        }
    }

    pub fn add_mapping(
        &mut self,
        start: u64,
        length: u64,
        readable: bool,
        writable: bool,
    ) -> Result<(), UserCopyError> {
        if self.mapping_count >= MAX_USER_COPY_MAPPINGS {
            return Err(UserCopyError::TooManyMappings);
        }
        let mapping = UserMapping::new(start, length, readable, writable)?;
        let mut index = 0;
        while index < self.mapping_count {
            let existing = self.mappings[index].ok_or(UserCopyError::OutOfRange)?;
            if mapping.overlaps_range(existing.start, existing.end) {
                return Err(UserCopyError::OverlappingMapping);
            }
            index += 1;
        }
        self.mappings[self.mapping_count] = Some(mapping);
        self.mapping_count += 1;
        Ok(())
    }

    pub fn validate_range(
        &self,
        ptr: u64,
        length: u64,
        access: UserCopyAccess,
    ) -> Result<ValidatedUserRange, UserCopyError> {
        if ptr == 0 {
            return Err(UserCopyError::NullPointer);
        }
        if length == 0 {
            return Err(UserCopyError::EmptyRange);
        }
        let end = ptr
            .checked_add(length)
            .ok_or(UserCopyError::LengthOverflow)?;
        let mapping_index = self
            .find_mapping_containing_start(ptr)
            .ok_or(UserCopyError::OutOfRange)?;
        let mapping = self.mappings[mapping_index].ok_or(UserCopyError::OutOfRange)?;
        if !mapping.contains_range(ptr, end) {
            if self.crosses_another_mapping(mapping_index, ptr, end) {
                return Err(UserCopyError::CrossMapping);
            }
            return Err(UserCopyError::OutOfRange);
        }
        if !mapping.allows(access) {
            return Err(UserCopyError::PermissionDenied);
        }
        Ok(ValidatedUserRange::new(ptr, end, access))
    }

    fn find_mapping_containing_start(&self, ptr: u64) -> Option<usize> {
        let mut index = 0;
        while index < self.mapping_count {
            if let Some(mapping) = self.mappings[index]
                && mapping.contains_start(ptr)
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn crosses_another_mapping(&self, current_index: usize, ptr: u64, end: u64) -> bool {
        let mut index = 0;
        while index < self.mapping_count {
            if index != current_index
                && let Some(mapping) = self.mappings[index]
                && mapping.overlaps_range(ptr, end)
            {
                return true;
            }
            index += 1;
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserCopyProof {
    pub valid_range: bool,
    pub out_of_range_denied: bool,
    pub length_overflow_denied: bool,
    pub cross_mapping_denied: bool,
}

pub fn run_self_test() -> Result<UserCopyProof, UserCopyError> {
    let mut map = UserCopyMap::new();
    map.add_mapping(0x0040_0000, 0x1000, true, false)?;
    map.add_mapping(0x0040_1000, 0x1000, true, true)?;

    let valid_range = map
        .validate_range(0x0040_0020, 64, UserCopyAccess::Read)
        .map(|range| {
            range.start() == 0x0040_0020
                && range.end() == 0x0040_0060
                && range.length() == 64
                && range.access() == UserCopyAccess::Read
        })?;
    let out_of_range_denied =
        map.validate_range(0x0060_0000, 32, UserCopyAccess::Read) == Err(UserCopyError::OutOfRange);
    let length_overflow_denied = map.validate_range(u64::MAX - 7, 16, UserCopyAccess::Read)
        == Err(UserCopyError::LengthOverflow);
    let cross_mapping_denied = map.validate_range(0x0040_0FF0, 32, UserCopyAccess::Read)
        == Err(UserCopyError::CrossMapping);
    let permission_denied = map.validate_range(0x0040_0020, 16, UserCopyAccess::Write)
        == Err(UserCopyError::PermissionDenied);

    if !valid_range {
        return Err(UserCopyError::OutOfRange);
    }
    if !out_of_range_denied {
        return Err(UserCopyError::OutOfRange);
    }
    if !length_overflow_denied {
        return Err(UserCopyError::LengthOverflow);
    }
    if !cross_mapping_denied {
        return Err(UserCopyError::CrossMapping);
    }
    if !permission_denied {
        return Err(UserCopyError::PermissionDenied);
    }

    Ok(UserCopyProof {
        valid_range,
        out_of_range_denied,
        length_overflow_denied,
        cross_mapping_denied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof_map() -> UserCopyMap {
        let mut map = UserCopyMap::new();
        map.add_mapping(0x0040_0000, 0x1000, true, false).unwrap();
        map.add_mapping(0x0040_1000, 0x1000, true, true).unwrap();
        map
    }

    #[test]
    fn valid_read_range_inside_one_mapping_is_accepted() {
        let map = proof_map();

        let range = map
            .validate_range(0x0040_0020, 64, UserCopyAccess::Read)
            .unwrap();

        assert_eq!(range.start(), 0x0040_0020);
        assert_eq!(range.end(), 0x0040_0060);
        assert_eq!(range.length(), 64);
    }

    #[test]
    fn pointer_outside_user_mapping_is_denied() {
        let map = proof_map();

        assert_eq!(
            map.validate_range(0x0060_0000, 32, UserCopyAccess::Read),
            Err(UserCopyError::OutOfRange)
        );
    }

    #[test]
    fn pointer_length_overflow_is_denied_before_lookup() {
        let map = proof_map();

        assert_eq!(
            map.validate_range(u64::MAX - 7, 16, UserCopyAccess::Read),
            Err(UserCopyError::LengthOverflow)
        );
    }

    #[test]
    fn range_crossing_into_another_mapping_is_denied() {
        let map = proof_map();

        assert_eq!(
            map.validate_range(0x0040_0FF0, 32, UserCopyAccess::Read),
            Err(UserCopyError::CrossMapping)
        );
    }

    #[test]
    fn write_requires_writable_mapping() {
        let map = proof_map();

        assert_eq!(
            map.validate_range(0x0040_0020, 16, UserCopyAccess::Write),
            Err(UserCopyError::PermissionDenied)
        );
        assert!(
            map.validate_range(0x0040_1020, 16, UserCopyAccess::Write)
                .is_ok()
        );
    }

    #[test]
    fn self_test_proves_required_copy_policy_cases() {
        let proof = run_self_test().unwrap();

        assert!(proof.valid_range);
        assert!(proof.out_of_range_denied);
        assert!(proof.length_overflow_denied);
        assert!(proof.cross_mapping_denied);
    }
}
