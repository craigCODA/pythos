//! Selection and backing identity policy for retained user roots.

pub(crate) fn for_selected_user_stack<E>(
    selected: Option<usize>,
    mut map: impl FnMut(crate::user_stacks::UserStackRegion) -> Result<(), E>,
) -> Result<(), ()> {
    let regions = crate::user_stacks::regions();
    if selected.is_some_and(|index| index >= regions.len()) {
        return Err(());
    }
    for (index, region) in regions.into_iter().enumerate() {
        if selected.is_none_or(|selected| selected == index) {
            map(region).map_err(|_| ())?;
        }
    }
    Ok(())
}

#[cfg(any(test, all(feature = "normal-session", not(feature = "verify"))))]
pub(crate) fn validate_disjoint_user_frames(own: &[u64], excluded: &[u64]) -> Result<(), ()> {
    if own.iter().any(|frame| excluded.contains(frame)) {
        Err(())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_stack_selection_maps_one_distinct_stack_and_rejects_invalid_selection() {
        for selected in 0..2 {
            let mut mapped = std::vec::Vec::new();
            for_selected_user_stack(Some(selected), |region| {
                mapped.push(region);
                Ok::<(), ()>(())
            })
            .unwrap();
            assert_eq!(mapped, [crate::user_stacks::regions()[selected]]);
        }
        assert!(
            for_selected_user_stack(Some(2), |_| -> Result<(), ()> {
                panic!("invalid selection mapped")
            })
            .is_err()
        );
        let mut count = 0;
        for_selected_user_stack(None, |_| {
            count += 1;
            Ok::<(), ()>(())
        })
        .unwrap();
        assert_eq!(count, 2);
    }
    #[test]
    fn normal_recovery_frame_identity_rejects_alias_even_with_different_virtual_layouts() {
        assert!(validate_disjoint_user_frames(&[0x1000, 0x2000], &[0x3000, 0x4000]).is_ok());
        assert!(validate_disjoint_user_frames(&[0x1000, 0x2000], &[0x4000, 0x1000]).is_err());
    }
}
