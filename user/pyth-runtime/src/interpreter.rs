use crate::value::Value;
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_runtime_abi::{
        GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_RUNTIME_ERROR, GraphExitRecord,
        MAX_PYTH_GRAPH_IMPORTS,
    },
    pyth_tig::{
        NO_VALUE,
        format::{MAX_RUNTIME_VALUES, PythGraphPackage},
        opcode::Opcode,
        types::PythType,
        verify::VerifiedGraph,
    },
};

pub use pythos_shared::pyth_runtime_abi::{GRAPH_EXIT_OK, GRAPH_RESULT_UNIT};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostError {
    Denied,
    Failed,
}

pub trait Host {
    fn system_log(&mut self, capability: PackedCapability, text: &[u8]) -> Result<(), HostError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    BudgetExhausted,
    InvalidBlock,
    InvalidNode,
    InvalidInput,
    InvalidValue,
    InvalidString,
    MissingImport,
    UnsupportedOpcode,
    Host(HostError),
}

pub struct Interpreter<'a> {
    graph: VerifiedGraph<'a>,
    imports: &'a [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
    values: [Option<Value>; MAX_RUNTIME_VALUES],
    budget: u64,
    executed_nodes: u64,
    last_node: u32,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        graph: VerifiedGraph<'a>,
        imports: &'a [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
        budget: u64,
    ) -> Self {
        Self {
            graph,
            imports,
            values: [None; MAX_RUNTIME_VALUES],
            budget,
            executed_nodes: 0,
            last_node: NO_VALUE,
        }
    }

    pub fn execute(mut self, host: &mut impl Host) -> GraphExitRecord {
        match self.execute_inner(host) {
            Ok(()) => self.exit_ok(),
            Err(RuntimeError::BudgetExhausted) => self.exit_budget_exhausted(),
            Err(error) => self.exit_runtime_error(error),
        }
    }

    fn execute_inner(&mut self, host: &mut impl Host) -> Result<(), RuntimeError> {
        let package = *self.graph.package();
        let entry_block = usize::try_from(package.header().entry_block)
            .map_err(|_| RuntimeError::InvalidBlock)?;
        self.execute_block(&package, entry_block, host)
    }

    fn execute_block(
        &mut self,
        package: &PythGraphPackage<'a>,
        block_index: usize,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let block = package
            .blocks()
            .get(block_index)
            .ok_or(RuntimeError::InvalidBlock)?;
        let first = usize::try_from(block.first_node).map_err(|_| RuntimeError::InvalidBlock)?;
        let count = usize::try_from(block.node_count).map_err(|_| RuntimeError::InvalidBlock)?;
        let end = first.checked_add(count).ok_or(RuntimeError::InvalidBlock)?;

        for node_index in first..end {
            self.dispatch_node(package, node_index, host)?;
            let node = package
                .nodes()
                .get(node_index)
                .ok_or(RuntimeError::InvalidNode)?;
            if Opcode::try_from(node.opcode).map_err(|_| RuntimeError::UnsupportedOpcode)?
                == Opcode::Return
            {
                return Ok(());
            }
        }

        Err(RuntimeError::InvalidBlock)
    }

    fn dispatch_node(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        if self.budget == 0 {
            self.last_node = u32::try_from(node_index).unwrap_or(NO_VALUE);
            return Err(RuntimeError::BudgetExhausted);
        }
        self.budget -= 1;
        self.executed_nodes = self
            .executed_nodes
            .checked_add(1)
            .ok_or(RuntimeError::InvalidValue)?;
        self.last_node = u32::try_from(node_index).map_err(|_| RuntimeError::InvalidNode)?;

        let node = package
            .nodes()
            .get(node_index)
            .ok_or(RuntimeError::InvalidNode)?;
        let opcode = Opcode::try_from(node.opcode).map_err(|_| RuntimeError::UnsupportedOpcode)?;
        match opcode {
            Opcode::BlockParam => self.execute_block_param(node_index, &node),
            Opcode::EffectStart => self.store_value(node_index, Value::Effect(node_index as u64)),
            Opcode::ConstUtf8 => self.execute_const_utf8(package, node_index, &node),
            Opcode::SystemLog => self.execute_system_log(package, node_index, &node, host),
            Opcode::Return => Ok(()),
            _ => Err(RuntimeError::UnsupportedOpcode),
        }
    }

    fn execute_block_param(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        let result_type =
            PythType::try_from(node.result_type).map_err(|_| RuntimeError::InvalidValue)?;
        if result_type != PythType::Capability {
            return Err(RuntimeError::UnsupportedOpcode);
        }
        let import_slot =
            usize::try_from(node.auxiliary0).map_err(|_| RuntimeError::MissingImport)?;
        let capability = *self
            .imports
            .get(import_slot)
            .ok_or(RuntimeError::MissingImport)?;
        if capability.raw() == 0 {
            return Err(RuntimeError::MissingImport);
        }
        self.store_value(node_index, Value::Capability(capability))
    }

    fn execute_const_utf8(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        let len = u16::try_from(node.auxiliary1).map_err(|_| RuntimeError::InvalidString)?;
        package
            .string_at(node.auxiliary0, len)
            .map_err(|_| RuntimeError::InvalidString)?;
        self.store_value(
            node_index,
            Value::Slice {
                offset: node.auxiliary0,
                len: node.auxiliary1,
                utf8: true,
            },
        )
    }

    fn execute_system_log(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let (offset, len) = self.expect_utf8_slice(node.input2)?;
        let text = package
            .string_at(
                offset,
                u16::try_from(len).map_err(|_| RuntimeError::InvalidString)?,
            )
            .map_err(|_| RuntimeError::InvalidString)?;
        host.system_log(capability, text)
            .map_err(RuntimeError::Host)?;
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn store_value(&mut self, node_index: usize, value: Value) -> Result<(), RuntimeError> {
        let slot = self
            .values
            .get_mut(node_index)
            .ok_or(RuntimeError::InvalidValue)?;
        *slot = Some(value);
        Ok(())
    }

    fn load_value(&self, node_index: u32) -> Result<Value, RuntimeError> {
        let index = usize::try_from(node_index).map_err(|_| RuntimeError::InvalidInput)?;
        self.values
            .get(index)
            .copied()
            .flatten()
            .ok_or(RuntimeError::InvalidInput)
    }

    fn expect_effect(&self, node_index: u32) -> Result<u64, RuntimeError> {
        match self.load_value(node_index)? {
            Value::Effect(effect) => Ok(effect),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_capability(&self, node_index: u32) -> Result<PackedCapability, RuntimeError> {
        match self.load_value(node_index)? {
            Value::Capability(capability) => Ok(capability),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_utf8_slice(&self, node_index: u32) -> Result<(u32, u32), RuntimeError> {
        match self.load_value(node_index)? {
            Value::Slice { offset, len, utf8 } if utf8 => Ok((offset, len)),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn exit_ok(&self) -> GraphExitRecord {
        GraphExitRecord {
            status: GRAPH_EXIT_OK,
            error_code: 0,
            last_node: self.last_node,
            executed_nodes: self.executed_nodes,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        }
    }

    fn exit_budget_exhausted(&self) -> GraphExitRecord {
        GraphExitRecord {
            status: GRAPH_EXIT_BUDGET_EXHAUSTED,
            error_code: RuntimeError::BudgetExhausted.code(),
            last_node: self.last_node,
            executed_nodes: self.executed_nodes,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        }
    }

    fn exit_runtime_error(&self, error: RuntimeError) -> GraphExitRecord {
        GraphExitRecord {
            status: GRAPH_EXIT_RUNTIME_ERROR,
            error_code: error.code(),
            last_node: self.last_node,
            executed_nodes: self.executed_nodes,
            result_type: GRAPH_RESULT_UNIT,
            reserved0: 0,
            reserved1: 0,
            result_raw: 0,
        }
    }
}

impl RuntimeError {
    pub const fn code(self) -> u16 {
        match self {
            Self::BudgetExhausted => 1,
            Self::InvalidBlock => 2,
            Self::InvalidNode => 3,
            Self::InvalidInput => 4,
            Self::InvalidValue => 5,
            Self::InvalidString => 6,
            Self::MissingImport => 7,
            Self::UnsupportedOpcode => 8,
            Self::Host(HostError::Denied) => 9,
            Self::Host(HostError::Failed) => 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_runtime_abi::MAX_PYTH_GRAPH_IMPORTS,
        pyth_tig::{format::PythGraphPackage, test_support, verify::verify_package},
    };

    struct RecordingHost {
        logs: [[u8; 16]; 4],
        log_count: usize,
    }

    impl Host for RecordingHost {
        fn system_log(
            &mut self,
            _capability: PackedCapability,
            text: &[u8],
        ) -> Result<(), HostError> {
            let mut slot = [0u8; 16];
            slot[..text.len()].copy_from_slice(text);
            self.logs[self.log_count] = slot;
            self.log_count += 1;
            Ok(())
        }
    }

    #[test]
    fn minimal_log_graph_executes_once_and_returns_unit() {
        let bytes = test_support::system_log_with_import_capability();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
        };
        let imports = [PackedCapability::from_parts(7, 1); MAX_PYTH_GRAPH_IMPORTS];

        let exit = Interpreter::new(verified, &imports, 64).execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(exit.executed_nodes, 5);
        assert_eq!(exit.result_type, GRAPH_RESULT_UNIT);
        assert_eq!(host.log_count, 1);
        assert_eq!(&host.logs[0][..5], b"hello");
    }
}
