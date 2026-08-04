use crate::pyth_tig::{
    NO_VALUE,
    format::{MAX_BLOCKS, MAX_GRAPH_NODES, PackageDecodeError, PythGraphPackage},
    opcode::Opcode,
    types::PythType,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyError {
    Decode(PackageDecodeError),
    UnknownType { code: u16 },
    UnknownOpcode { code: u16 },
    InvalidBlockRange { block: u32 },
    MissingTerminator { block: u32 },
    MultipleTerminators { block: u32 },
    InvalidControlTarget { block: u32, target: u32 },
    BlockArgumentCountMismatch { source: u32, target: u32 },
    ValueNotAvailable { node: u32, input: u8 },
    ResultTypeForbidden { node: u32 },
    ResourceBudgetExceeded,
    NonCanonicalEncoding,
    ChecksumMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedGraph<'a> {
    package: PythGraphPackage<'a>,
}

impl<'a> VerifiedGraph<'a> {
    pub const fn package(&self) -> &PythGraphPackage<'a> {
        &self.package
    }
}

pub fn verify_bytes(bytes: &[u8]) -> Result<VerifiedGraph<'_>, VerifyError> {
    let package = PythGraphPackage::decode(bytes).map_err(VerifyError::Decode)?;
    verify_package(&package)
}

pub fn verify_package<'a>(
    package: &PythGraphPackage<'a>,
) -> Result<VerifiedGraph<'a>, VerifyError> {
    let block_count = package.blocks().len();
    let node_count = package.nodes().len();

    if block_count > MAX_BLOCKS || node_count > MAX_GRAPH_NODES {
        return Err(VerifyError::ResourceBudgetExceeded);
    }

    verify_known_types(package)?;
    verify_known_opcodes(package)?;

    let mut block_starts = [0usize; MAX_BLOCKS];
    let mut block_ends = [0usize; MAX_BLOCKS];
    let mut node_blocks = [usize::MAX; MAX_GRAPH_NODES];
    verify_block_ranges(
        package,
        &mut block_starts,
        &mut block_ends,
        &mut node_blocks,
    )?;
    verify_control_flow(package, block_starts, block_ends)?;
    verify_value_availability(package, node_blocks)?;

    Ok(VerifiedGraph { package: *package })
}

fn verify_known_types(package: &PythGraphPackage<'_>) -> Result<(), VerifyError> {
    for ty in package.types().iter() {
        PythType::try_from(ty.kind)
            .map_err(|unknown| VerifyError::UnknownType { code: unknown.code })?;
    }

    for node in package.nodes().iter() {
        PythType::try_from(node.result_type)
            .map_err(|unknown| VerifyError::UnknownType { code: unknown.code })?;
    }

    Ok(())
}

fn verify_known_opcodes(package: &PythGraphPackage<'_>) -> Result<(), VerifyError> {
    for node in package.nodes().iter() {
        Opcode::try_from(node.opcode)
            .map_err(|unknown| VerifyError::UnknownOpcode { code: unknown.code })?;
    }

    Ok(())
}

fn verify_block_ranges(
    package: &PythGraphPackage<'_>,
    block_starts: &mut [usize; MAX_BLOCKS],
    block_ends: &mut [usize; MAX_BLOCKS],
    node_blocks: &mut [usize; MAX_GRAPH_NODES],
) -> Result<(), VerifyError> {
    let node_count = package.nodes().len();

    for (block_index, block) in package.blocks().iter().enumerate() {
        let first =
            usize::try_from(block.first_node).map_err(|_| VerifyError::InvalidBlockRange {
                block: block.block_id,
            })?;
        let count =
            usize::try_from(block.node_count).map_err(|_| VerifyError::InvalidBlockRange {
                block: block.block_id,
            })?;
        let end = first
            .checked_add(count)
            .ok_or(VerifyError::InvalidBlockRange {
                block: block.block_id,
            })?;

        if block.block_id != block_index as u32 || count == 0 || end > node_count {
            return Err(VerifyError::InvalidBlockRange {
                block: block.block_id,
            });
        }

        block_starts[block_index] = first;
        block_ends[block_index] = end;

        let mut terminators = 0usize;
        for (node_index, node_block) in node_blocks.iter_mut().enumerate().take(end).skip(first) {
            if *node_block != usize::MAX {
                return Err(VerifyError::InvalidBlockRange {
                    block: block.block_id,
                });
            }
            let Some(node) = package.nodes().get(node_index) else {
                return Err(VerifyError::InvalidBlockRange {
                    block: block.block_id,
                });
            };
            if usize::from(node.block_index) != block_index {
                return Err(VerifyError::InvalidBlockRange {
                    block: block.block_id,
                });
            }
            *node_block = block_index;

            let opcode = Opcode::try_from(node.opcode)
                .expect("known opcode validation already accepted every node opcode");
            if is_terminator(opcode) {
                terminators += 1;
                if node_index + 1 != end {
                    return Err(VerifyError::MultipleTerminators {
                        block: block.block_id,
                    });
                }
            }
        }

        if terminators == 0 {
            return Err(VerifyError::MissingTerminator {
                block: block.block_id,
            });
        }
        if terminators > 1 {
            return Err(VerifyError::MultipleTerminators {
                block: block.block_id,
            });
        }

        let expected_terminator = end - 1;
        if block.terminator_node != expected_terminator as u32 {
            return Err(VerifyError::MissingTerminator {
                block: block.block_id,
            });
        }
    }

    Ok(())
}

fn verify_control_flow(
    package: &PythGraphPackage<'_>,
    block_starts: [usize; MAX_BLOCKS],
    block_ends: [usize; MAX_BLOCKS],
) -> Result<(), VerifyError> {
    let block_count = package.blocks().len();

    for block_index in 0..block_count {
        let block = package
            .blocks()
            .get(block_index)
            .expect("validated block index remains decodable");
        let terminator_index = block_ends[block_index] - 1;
        let terminator = package
            .nodes()
            .get(terminator_index)
            .expect("validated terminator node remains decodable");
        let opcode = Opcode::try_from(terminator.opcode)
            .expect("known opcode validation already accepted every node opcode");

        match opcode {
            Opcode::Jump => {
                validate_target(package, block.block_id, terminator.auxiliary0)?;
                validate_jump_arity(package, &terminator, block.block_id, terminator.auxiliary0)?;
            }
            Opcode::Branch => {
                validate_target(package, block.block_id, terminator.auxiliary0)?;
                validate_target(package, block.block_id, terminator.auxiliary1)?;
                validate_branch_arity(package, block.block_id, terminator.auxiliary0)?;
                validate_branch_arity(package, block.block_id, terminator.auxiliary1)?;
            }
            Opcode::Return => {}
            _ => {
                return Err(VerifyError::MissingTerminator {
                    block: block.block_id,
                });
            }
        }

        for node_index in block_starts[block_index]..terminator_index {
            let node = package
                .nodes()
                .get(node_index)
                .expect("validated node index remains decodable");
            let opcode = Opcode::try_from(node.opcode)
                .expect("known opcode validation already accepted every node opcode");
            if is_terminator(opcode) {
                return Err(VerifyError::MultipleTerminators {
                    block: block.block_id,
                });
            }
        }
    }

    Ok(())
}

fn validate_target(
    package: &PythGraphPackage<'_>,
    source: u32,
    target: u32,
) -> Result<(), VerifyError> {
    let target_index = usize::try_from(target).map_err(|_| VerifyError::InvalidControlTarget {
        block: source,
        target,
    })?;
    if target_index >= package.blocks().len() {
        return Err(VerifyError::InvalidControlTarget {
            block: source,
            target,
        });
    }
    Ok(())
}

fn validate_jump_arity(
    package: &PythGraphPackage<'_>,
    terminator: &crate::pyth_tig::NodeRecord,
    source: u32,
    target: u32,
) -> Result<(), VerifyError> {
    let target_index = usize::try_from(target).map_err(|_| VerifyError::InvalidControlTarget {
        block: source,
        target,
    })?;
    let target_block = package
        .blocks()
        .get(target_index)
        .expect("validated control target remains decodable");
    let provided = count_values([
        terminator.input0,
        terminator.input1,
        terminator.input2,
        terminator.input3,
    ]);
    if provided != usize::from(target_block.parameter_count) {
        return Err(VerifyError::BlockArgumentCountMismatch { source, target });
    }
    Ok(())
}

fn validate_branch_arity(
    package: &PythGraphPackage<'_>,
    source: u32,
    target: u32,
) -> Result<(), VerifyError> {
    let target_index = usize::try_from(target).map_err(|_| VerifyError::InvalidControlTarget {
        block: source,
        target,
    })?;
    let target_block = package
        .blocks()
        .get(target_index)
        .expect("validated control target remains decodable");
    if target_block.parameter_count != 0 {
        return Err(VerifyError::BlockArgumentCountMismatch { source, target });
    }
    Ok(())
}

fn verify_value_availability(
    package: &PythGraphPackage<'_>,
    node_blocks: [usize; MAX_GRAPH_NODES],
) -> Result<(), VerifyError> {
    let dominators = compute_dominators(package)?;
    for (node_index, node) in package.nodes().iter().enumerate() {
        let inputs = [node.input0, node.input1, node.input2, node.input3];
        for (input_index, input) in inputs.into_iter().enumerate() {
            if input == NO_VALUE {
                continue;
            }
            let producer_index =
                usize::try_from(input).map_err(|_| VerifyError::ValueNotAvailable {
                    node: node_index as u32,
                    input: input_index as u8,
                })?;
            if producer_index >= package.nodes().len() {
                return Err(VerifyError::ValueNotAvailable {
                    node: node_index as u32,
                    input: input_index as u8,
                });
            }

            let consumer_block = node_blocks[node_index];
            let producer_block = node_blocks[producer_index];
            if consumer_block == usize::MAX || producer_block == usize::MAX {
                return Err(VerifyError::ValueNotAvailable {
                    node: node_index as u32,
                    input: input_index as u8,
                });
            }

            let available = if producer_block == consumer_block {
                producer_index < node_index
            } else {
                (dominators[consumer_block] & block_bit(producer_block)) != 0
            };
            if !available {
                return Err(VerifyError::ValueNotAvailable {
                    node: node_index as u32,
                    input: input_index as u8,
                });
            }
        }
    }

    Ok(())
}

fn compute_dominators(package: &PythGraphPackage<'_>) -> Result<[u128; MAX_BLOCKS], VerifyError> {
    let block_count = package.blocks().len();
    let entry = usize::try_from(package.header().entry_block).map_err(|_| {
        VerifyError::InvalidControlTarget {
            block: package.header().entry_block,
            target: package.header().entry_block,
        }
    })?;
    if entry >= block_count {
        return Err(VerifyError::InvalidControlTarget {
            block: package.header().entry_block,
            target: package.header().entry_block,
        });
    }

    let mut predecessors = [0u128; MAX_BLOCKS];
    for block_index in 0..block_count {
        let block = package
            .blocks()
            .get(block_index)
            .expect("validated block index remains decodable");
        let terminator_index =
            usize::try_from(block.terminator_node).map_err(|_| VerifyError::InvalidBlockRange {
                block: block.block_id,
            })?;
        let terminator =
            package
                .nodes()
                .get(terminator_index)
                .ok_or(VerifyError::InvalidBlockRange {
                    block: block.block_id,
                })?;
        let opcode = Opcode::try_from(terminator.opcode)
            .expect("known opcode validation already accepted every node opcode");
        match opcode {
            Opcode::Jump => {
                let target = usize::try_from(terminator.auxiliary0).map_err(|_| {
                    VerifyError::InvalidControlTarget {
                        block: block.block_id,
                        target: terminator.auxiliary0,
                    }
                })?;
                predecessors[target] |= block_bit(block_index);
            }
            Opcode::Branch => {
                let true_target = usize::try_from(terminator.auxiliary0).map_err(|_| {
                    VerifyError::InvalidControlTarget {
                        block: block.block_id,
                        target: terminator.auxiliary0,
                    }
                })?;
                let false_target = usize::try_from(terminator.auxiliary1).map_err(|_| {
                    VerifyError::InvalidControlTarget {
                        block: block.block_id,
                        target: terminator.auxiliary1,
                    }
                })?;
                predecessors[true_target] |= block_bit(block_index);
                predecessors[false_target] |= block_bit(block_index);
            }
            Opcode::Return => {}
            _ => {}
        }
    }

    let all_blocks = if block_count == 128 {
        u128::MAX
    } else {
        (1u128 << block_count) - 1
    };
    let mut dominators = [0u128; MAX_BLOCKS];
    for (block_index, dominator) in dominators.iter_mut().enumerate().take(block_count) {
        *dominator = if block_index == entry {
            block_bit(block_index)
        } else {
            all_blocks
        };
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block_index in 0..block_count {
            if block_index == entry {
                continue;
            }

            let preds = predecessors[block_index];
            let mut intersection = all_blocks;
            for (pred, dominator) in dominators.iter().enumerate().take(block_count) {
                if (preds & block_bit(pred)) != 0 {
                    intersection &= *dominator;
                }
            }

            let next = block_bit(block_index) | intersection;
            if dominators[block_index] != next {
                dominators[block_index] = next;
                changed = true;
            }
        }
    }

    Ok(dominators)
}

const fn is_terminator(opcode: Opcode) -> bool {
    matches!(opcode, Opcode::Jump | Opcode::Branch | Opcode::Return)
}

const fn block_bit(block: usize) -> u128 {
    1u128 << block
}

fn count_values(inputs: [u32; 4]) -> usize {
    let mut count = 0usize;
    for input in inputs {
        if input != NO_VALUE {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_tig::test_support;

    #[test]
    fn verifier_rejects_missing_terminator_bad_target_and_use_before_definition() {
        let missing = test_support::package_without_terminator();
        assert_eq!(
            verify_bytes(&missing),
            Err(VerifyError::MissingTerminator { block: 0 })
        );

        let target = test_support::package_with_bad_branch_target();
        assert_eq!(
            verify_bytes(&target),
            Err(VerifyError::InvalidControlTarget {
                block: 0,
                target: 9
            })
        );

        let use_before = test_support::package_with_use_before_definition();
        assert_eq!(
            verify_bytes(&use_before),
            Err(VerifyError::ValueNotAvailable { node: 1, input: 0 })
        );
    }
}
