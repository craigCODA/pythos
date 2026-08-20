use crate::value::Value;
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_command_abi::{
        COMMAND_FIELD_KIND, COMMAND_FIELD_OBJECT_ID, COMMAND_FIELD_PROPOSAL_ID,
        COMMAND_FIELD_TASK_ID, COMMAND_FIELD_TEXT_UTF8, command_kind_is_known,
    },
    pyth_runtime_abi::{
        GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_RUNTIME_ERROR, GraphExitRecord,
        HOST_RESULT_CAPABILITY, HOST_RESULT_OBJECT_ID, HOST_RESULT_REVISION, HOST_RESULT_STATUS,
        HOST_RESULT_UTF8, HostCallResult, MAX_HOST_RESULT_BYTES, MAX_PYTH_GRAPH_IMPORTS,
    },
    pyth_tig::{
        NO_VALUE,
        format::{MAX_RUNTIME_VALUES, PythGraphPackage},
        opcode::Opcode,
        types::PythType,
        verify::VerifiedGraph,
    },
    task_abi::{
        TASK_CONTEXT_RESULT_ACTIVE_TASK_ID, TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID,
        TASK_CONTEXT_RESULT_CONFIDENCE_SCORE, TASK_CONTEXT_RESULT_PROPOSAL_KIND,
        TASK_CONTEXT_RESULT_REASON_UTF8,
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
    fn object_create(
        &mut self,
        capability: PackedCapability,
        kind: &[u8],
    ) -> Result<HostCallResult, HostError>;
    fn object_query(
        &mut self,
        capability: PackedCapability,
        kind: &[u8],
    ) -> Result<HostCallResult, HostError>;
    fn object_inspect(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
    ) -> Result<HostCallResult, HostError>;
    fn object_revise(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
        text: &[u8],
    ) -> Result<HostCallResult, HostError>;
    fn object_history(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
    ) -> Result<HostCallResult, HostError>;
    fn task_context(&mut self, capability: PackedCapability) -> Result<HostCallResult, HostError>;
    fn task_propose(
        &mut self,
        capability: PackedCapability,
        candidate_task_id: u64,
        score: u64,
    ) -> Result<(), HostError>;
    fn command_read(&mut self, capability: PackedCapability) -> Result<HostCallResult, HostError>;
    fn command_result_emit(
        &mut self,
        capability: PackedCapability,
        status: u16,
        text: &[u8],
    ) -> Result<(), HostError>;
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
    values: &'a mut [Option<Value>; MAX_RUNTIME_VALUES],
    host_results: &'a mut [Option<HostCallResult>; MAX_RUNTIME_VALUES],
    budget: u64,
    executed_nodes: u64,
    last_node: u32,
}

enum BlockOutcome {
    Return,
    Jump(usize),
}

impl<'a> Interpreter<'a> {
    pub fn new(
        graph: VerifiedGraph<'a>,
        imports: &'a [PackedCapability; MAX_PYTH_GRAPH_IMPORTS],
        budget: u64,
        values: &'a mut [Option<Value>; MAX_RUNTIME_VALUES],
        host_results: &'a mut [Option<HostCallResult>; MAX_RUNTIME_VALUES],
    ) -> Self {
        values.fill(None);
        host_results.fill(None);
        Self {
            graph,
            imports,
            values,
            host_results,
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
        let mut block_index = usize::try_from(package.header().entry_block)
            .map_err(|_| RuntimeError::InvalidBlock)?;
        loop {
            match self.execute_block(&package, block_index, host)? {
                BlockOutcome::Return => return Ok(()),
                BlockOutcome::Jump(target) => block_index = target,
            }
        }
    }

    fn execute_block(
        &mut self,
        package: &PythGraphPackage<'a>,
        block_index: usize,
        host: &mut impl Host,
    ) -> Result<BlockOutcome, RuntimeError> {
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
                return Ok(BlockOutcome::Return);
            }
            if Opcode::try_from(node.opcode).map_err(|_| RuntimeError::UnsupportedOpcode)?
                == Opcode::Jump
            {
                let target =
                    usize::try_from(node.auxiliary0).map_err(|_| RuntimeError::InvalidBlock)?;
                if package.blocks().get(target).is_none() {
                    return Err(RuntimeError::InvalidBlock);
                }
                return Ok(BlockOutcome::Jump(target));
            }
            if Opcode::try_from(node.opcode).map_err(|_| RuntimeError::UnsupportedOpcode)?
                == Opcode::Branch
            {
                let target = if self.expect_bool(node.input0)? {
                    node.auxiliary0
                } else {
                    node.auxiliary1
                };
                let target = usize::try_from(target).map_err(|_| RuntimeError::InvalidBlock)?;
                if package.blocks().get(target).is_none() {
                    return Err(RuntimeError::InvalidBlock);
                }
                return Ok(BlockOutcome::Jump(target));
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
            Opcode::ConstBool => self.execute_const_bool(node_index, &node),
            Opcode::ConstU64 => self.execute_const_u64(node_index, &node),
            Opcode::LessThanU64 => self.execute_less_than_u64(node_index, &node),
            Opcode::ConstBytes => self.execute_const_bytes(package, node_index, &node),
            Opcode::ConstUtf8 => self.execute_const_utf8(package, node_index, &node),
            Opcode::HostResult => self.execute_host_result(package, node_index, &node),
            Opcode::SystemLog => self.execute_system_log(package, node_index, &node, host),
            Opcode::ObjectCreate => self.execute_object_create(package, node_index, &node, host),
            Opcode::ObjectQuery => self.execute_object_query(package, node_index, &node, host),
            Opcode::ObjectInspect => self.execute_object_inspect(node_index, &node, host),
            Opcode::ObjectRevise => self.execute_object_revise(package, node_index, &node, host),
            Opcode::ObjectHistory => self.execute_object_history(node_index, &node, host),
            Opcode::TaskContextRead => self.execute_task_context(node_index, &node, host),
            Opcode::TaskProposalEmit => self.execute_task_proposal_emit(node_index, &node, host),
            Opcode::CommandRead => self.execute_command_read(node_index, &node, host),
            Opcode::CommandResultEmit => {
                self.execute_command_result_emit(package, node_index, &node, host)
            }
            Opcode::Jump => Ok(()),
            Opcode::Branch => Ok(()),
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

    fn execute_const_bool(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        if PythType::try_from(node.result_type).map_err(|_| RuntimeError::InvalidValue)?
            != PythType::Bool
        {
            return Err(RuntimeError::UnsupportedOpcode);
        }
        match node.immediate {
            0 => self.store_value(node_index, Value::Bool(false)),
            1 => self.store_value(node_index, Value::Bool(true)),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn execute_const_u64(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        match PythType::try_from(node.result_type).map_err(|_| RuntimeError::InvalidValue)? {
            PythType::U64 => self.store_value(node_index, Value::U64(node.immediate)),
            PythType::ObjectId => self.store_value(node_index, Value::ObjectId(node.immediate)),
            PythType::RevisionId => self.store_value(node_index, Value::RevisionId(node.immediate)),
            PythType::TaskId => self.store_value(node_index, Value::TaskId(node.immediate)),
            PythType::ProposalId => self.store_value(node_index, Value::ProposalId(node.immediate)),
            PythType::ErrorCode => self.store_value(
                node_index,
                Value::ErrorCode(
                    u16::try_from(node.immediate).map_err(|_| RuntimeError::InvalidValue)?,
                ),
            ),
            _ => Err(RuntimeError::UnsupportedOpcode),
        }
    }

    fn execute_less_than_u64(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        if PythType::try_from(node.result_type).map_err(|_| RuntimeError::InvalidValue)?
            != PythType::Bool
        {
            return Err(RuntimeError::UnsupportedOpcode);
        }
        let lhs = self.expect_u64(node.input0)?;
        let rhs = self.expect_u64(node.input1)?;
        self.store_value(node_index, Value::Bool(lhs < rhs))
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

    fn execute_const_bytes(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        let offset = usize::try_from(node.auxiliary0).map_err(|_| RuntimeError::InvalidString)?;
        let len = usize::try_from(node.auxiliary1).map_err(|_| RuntimeError::InvalidString)?;
        let end = offset.checked_add(len).ok_or(RuntimeError::InvalidString)?;
        package
            .constant_pool()
            .get(offset..end)
            .ok_or(RuntimeError::InvalidString)?;
        self.store_value(
            node_index,
            Value::Slice {
                offset: node.auxiliary0,
                len: node.auxiliary1,
                utf8: false,
            },
        )
    }

    fn execute_host_result(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
    ) -> Result<(), RuntimeError> {
        let producer_index =
            usize::try_from(node.input0).map_err(|_| RuntimeError::InvalidInput)?;
        let producer = package
            .nodes()
            .get(producer_index)
            .ok_or(RuntimeError::InvalidInput)?;
        let producer_opcode =
            Opcode::try_from(producer.opcode).map_err(|_| RuntimeError::UnsupportedOpcode)?;
        let result = self
            .host_results
            .get(producer_index)
            .copied()
            .flatten()
            .ok_or(RuntimeError::InvalidValue)?;
        let value = match producer_opcode {
            Opcode::TaskContextRead => {
                self.task_context_host_result_value(node.input0, node.auxiliary0, result)?
            }
            Opcode::CommandRead => {
                self.command_host_result_value(node.input0, node.auxiliary0, result)?
            }
            _ => match node.auxiliary0 {
                HOST_RESULT_STATUS => Value::ErrorCode(result.status),
                HOST_RESULT_OBJECT_ID => Value::ObjectId(result.object_id),
                HOST_RESULT_REVISION => Value::RevisionId(result.revision),
                HOST_RESULT_CAPABILITY => {
                    if result.capability.raw() == 0 {
                        return Err(RuntimeError::InvalidValue);
                    }
                    Value::Capability(result.capability)
                }
                HOST_RESULT_UTF8 => self.host_utf8_value(node.input0, result)?,
                _ => return Err(RuntimeError::InvalidValue),
            },
        };
        self.store_value(node_index, value)
    }

    fn task_context_host_result_value(
        &self,
        producer_node: u32,
        field: u32,
        result: HostCallResult,
    ) -> Result<Value, RuntimeError> {
        match field {
            TASK_CONTEXT_RESULT_ACTIVE_TASK_ID => Ok(Value::TaskId(result.object_id)),
            TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID => Ok(Value::TaskId(result.revision)),
            TASK_CONTEXT_RESULT_CONFIDENCE_SCORE => Ok(Value::U64(result.capability.raw())),
            TASK_CONTEXT_RESULT_PROPOSAL_KIND => Ok(Value::U64(u64::from(result.reserved0))),
            TASK_CONTEXT_RESULT_REASON_UTF8 => self.host_utf8_value(producer_node, result),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn command_host_result_value(
        &self,
        producer_node: u32,
        field: u32,
        result: HostCallResult,
    ) -> Result<Value, RuntimeError> {
        match field {
            COMMAND_FIELD_KIND => Ok(Value::U64(u64::from(result.reserved0))),
            COMMAND_FIELD_OBJECT_ID => Ok(Value::ObjectId(result.object_id)),
            COMMAND_FIELD_TASK_ID => Ok(Value::TaskId(result.revision)),
            COMMAND_FIELD_PROPOSAL_ID => Ok(Value::ProposalId(result.capability.raw())),
            COMMAND_FIELD_TEXT_UTF8 => self.host_utf8_value(producer_node, result),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn host_utf8_value(
        &self,
        producer_node: u32,
        result: HostCallResult,
    ) -> Result<Value, RuntimeError> {
        let len = usize::from(result.bytes_len);
        if len > MAX_HOST_RESULT_BYTES || core::str::from_utf8(&result.bytes[..len]).is_err() {
            return Err(RuntimeError::InvalidString);
        }
        Ok(Value::HostUtf8 {
            producer_node,
            len: result.bytes_len,
        })
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
        let text = self.expect_utf8_bytes(package, node.input2)?;
        host.system_log(capability, text)
            .map_err(RuntimeError::Host)?;
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn execute_object_create(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let kind = self.expect_utf8_bytes(package, node.input2)?;
        let result = host
            .object_create(capability, kind)
            .map_err(RuntimeError::Host)?;
        self.store_host_result(node_index, result)
    }

    fn execute_object_query(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let kind = self.expect_utf8_bytes(package, node.input2)?;
        let result = host
            .object_query(capability, kind)
            .map_err(RuntimeError::Host)?;
        self.store_host_result(node_index, result)
    }

    fn execute_object_inspect(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let object_id = self.expect_object_id(node.input2)?;
        let result = host
            .object_inspect(capability, object_id)
            .map_err(RuntimeError::Host)?;
        self.store_host_result(node_index, result)
    }

    fn execute_object_revise(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let object_id = self.expect_object_id(node.input2)?;
        let text = self.expect_bytes(package, node.input3)?;
        let result = host
            .object_revise(capability, object_id, text)
            .map_err(RuntimeError::Host)?;
        self.store_host_result(node_index, result)
    }

    fn execute_object_history(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let object_id = self.expect_object_id(node.input2)?;
        let result = host
            .object_history(capability, object_id)
            .map_err(RuntimeError::Host)?;
        self.store_host_result(node_index, result)
    }

    fn execute_task_context(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let result = host.task_context(capability).map_err(RuntimeError::Host)?;
        validate_task_context_result(result)?;
        let slot = self
            .host_results
            .get_mut(node_index)
            .ok_or(RuntimeError::InvalidValue)?;
        *slot = Some(result);
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn execute_task_proposal_emit(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let candidate_task_id = self.expect_task_id(node.input2)?;
        let score = self.expect_u64(node.input3)?;
        host.task_propose(capability, candidate_task_id, score)
            .map_err(RuntimeError::Host)?;
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn execute_command_read(
        &mut self,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let result = host.command_read(capability).map_err(RuntimeError::Host)?;
        validate_command_read_result(result)?;
        let slot = self
            .host_results
            .get_mut(node_index)
            .ok_or(RuntimeError::InvalidValue)?;
        *slot = Some(result);
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn execute_command_result_emit(
        &mut self,
        package: &PythGraphPackage<'a>,
        node_index: usize,
        node: &pythos_shared::pyth_tig::NodeRecord,
        host: &mut impl Host,
    ) -> Result<(), RuntimeError> {
        let _effect = self.expect_effect(node.input0)?;
        let capability = self.expect_capability(node.input1)?;
        let status = self.expect_error_code(node.input2)?;
        let text = self.expect_utf8_bytes(package, node.input3)?;
        host.command_result_emit(capability, status, text)
            .map_err(RuntimeError::Host)?;
        self.store_value(node_index, Value::Effect(node_index as u64))
    }

    fn store_host_result(
        &mut self,
        node_index: usize,
        result: HostCallResult,
    ) -> Result<(), RuntimeError> {
        validate_host_call_result(result)?;
        let slot = self
            .host_results
            .get_mut(node_index)
            .ok_or(RuntimeError::InvalidValue)?;
        *slot = Some(result);
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

    fn expect_utf8_bytes<'b>(
        &'b self,
        package: &'b PythGraphPackage<'a>,
        node_index: u32,
    ) -> Result<&'b [u8], RuntimeError> {
        match self.load_value(node_index)? {
            Value::Slice { offset, len, utf8 } if utf8 => package
                .string_at(
                    offset,
                    u16::try_from(len).map_err(|_| RuntimeError::InvalidString)?,
                )
                .map_err(|_| RuntimeError::InvalidString),
            Value::HostUtf8 { producer_node, len } => {
                let producer_index =
                    usize::try_from(producer_node).map_err(|_| RuntimeError::InvalidInput)?;
                let result = self
                    .host_results
                    .get(producer_index)
                    .and_then(Option::as_ref)
                    .ok_or(RuntimeError::InvalidValue)?;
                let len = usize::from(len);
                if len > MAX_HOST_RESULT_BYTES {
                    return Err(RuntimeError::InvalidString);
                }
                let bytes = &result.bytes[..len];
                core::str::from_utf8(bytes).map_err(|_| RuntimeError::InvalidString)?;
                Ok(bytes)
            }
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_bytes<'b>(
        &'b self,
        package: &'b PythGraphPackage<'a>,
        node_index: u32,
    ) -> Result<&'b [u8], RuntimeError> {
        match self.load_value(node_index)? {
            Value::Slice { offset, len, utf8 } if !utf8 => {
                let offset = usize::try_from(offset).map_err(|_| RuntimeError::InvalidString)?;
                let len = usize::try_from(len).map_err(|_| RuntimeError::InvalidString)?;
                let end = offset.checked_add(len).ok_or(RuntimeError::InvalidString)?;
                package
                    .constant_pool()
                    .get(offset..end)
                    .ok_or(RuntimeError::InvalidString)
            }
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_object_id(&self, node_index: u32) -> Result<u64, RuntimeError> {
        match self.load_value(node_index)? {
            Value::ObjectId(object_id) => Ok(object_id),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_task_id(&self, node_index: u32) -> Result<u64, RuntimeError> {
        match self.load_value(node_index)? {
            Value::TaskId(task_id) => Ok(task_id),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_u64(&self, node_index: u32) -> Result<u64, RuntimeError> {
        match self.load_value(node_index)? {
            Value::U64(value) => Ok(value),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_error_code(&self, node_index: u32) -> Result<u16, RuntimeError> {
        match self.load_value(node_index)? {
            Value::ErrorCode(value) => Ok(value),
            _ => Err(RuntimeError::InvalidValue),
        }
    }

    fn expect_bool(&self, node_index: u32) -> Result<bool, RuntimeError> {
        match self.load_value(node_index)? {
            Value::Bool(value) => Ok(value),
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

fn validate_host_call_result(result: HostCallResult) -> Result<(), RuntimeError> {
    if result.reserved0 != 0
        || result.reserved1 != [0; 16]
        || usize::from(result.bytes_len) > MAX_HOST_RESULT_BYTES
    {
        return Err(RuntimeError::InvalidValue);
    }
    Ok(())
}

fn validate_task_context_result(result: HostCallResult) -> Result<(), RuntimeError> {
    if result.reserved1 != [0; 16] || usize::from(result.bytes_len) > MAX_HOST_RESULT_BYTES {
        return Err(RuntimeError::InvalidValue);
    }
    let len = usize::from(result.bytes_len);
    if core::str::from_utf8(&result.bytes[..len]).is_err() {
        return Err(RuntimeError::InvalidString);
    }
    Ok(())
}

fn validate_command_read_result(result: HostCallResult) -> Result<(), RuntimeError> {
    let command_kind = u16::try_from(result.reserved0).map_err(|_| RuntimeError::InvalidValue)?;
    if result.reserved1 != [0; 16]
        || usize::from(result.bytes_len) > MAX_HOST_RESULT_BYTES
        || !command_kind_is_known(command_kind)
    {
        return Err(RuntimeError::InvalidValue);
    }
    let len = usize::from(result.bytes_len);
    if core::str::from_utf8(&result.bytes[..len]).is_err() {
        return Err(RuntimeError::InvalidString);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_command_abi::COMMAND_KIND_CREATE_NOTE,
        pyth_runtime_abi::{HOST_RESULT_STATUS, HostCallResult, MAX_PYTH_GRAPH_IMPORTS},
        pyth_tig::{
            format::PythGraphPackage,
            opcode::{RIGHTS_CREATE, RIGHTS_READ},
            test_support,
            verify::verify_package,
        },
        task_abi::TaskProposalKind,
    };

    struct RecordingHost {
        logs: [[u8; 16]; 4],
        log_count: usize,
        create_count: usize,
        revise_count: usize,
        inspect_count: usize,
        last_revise_capability: PackedCapability,
        last_inspect_capability: PackedCapability,
        last_text: [u8; 16],
        last_text_len: usize,
        malformed_create: bool,
        deny_create: bool,
        proposal_count: usize,
        last_candidate_task_id: u64,
        last_score: u64,
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

        fn object_create(
            &mut self,
            _capability: PackedCapability,
            kind: &[u8],
        ) -> Result<HostCallResult, HostError> {
            if kind != b"note" {
                return Err(HostError::Failed);
            }
            self.create_count += 1;
            if self.deny_create {
                return Ok(HostCallResult::empty(1));
            }
            let mut result = HostCallResult::empty(0);
            result.object_id = 1042;
            result.revision = 1;
            result.capability = PackedCapability::from_parts(9, 2);
            if self.malformed_create {
                result.reserved0 = 1;
            }
            Ok(result)
        }

        fn object_query(
            &mut self,
            _capability: PackedCapability,
            _kind: &[u8],
        ) -> Result<HostCallResult, HostError> {
            let mut result = HostCallResult::empty(0);
            result.object_id = 1042;
            result.capability = PackedCapability::from_parts(9, 2);
            Ok(result)
        }

        fn object_inspect(
            &mut self,
            capability: PackedCapability,
            object_id: u64,
        ) -> Result<HostCallResult, HostError> {
            if object_id != 1042 {
                self.inspect_count += 1;
                self.last_inspect_capability = capability;
                let mut result = HostCallResult::empty(1);
                result.object_id = object_id;
                return Ok(result);
            }
            self.inspect_count += 1;
            self.last_inspect_capability = capability;
            self.last_text[..5].copy_from_slice(b"hello");
            self.last_text_len = 5;
            let mut result = HostCallResult::empty(0);
            result.object_id = object_id;
            result.revision = 2;
            result.bytes_len = 5;
            result.bytes[..5].copy_from_slice(b"hello");
            Ok(result)
        }

        fn object_revise(
            &mut self,
            capability: PackedCapability,
            object_id: u64,
            text: &[u8],
        ) -> Result<HostCallResult, HostError> {
            if object_id != 1042 || text != b"hello" {
                return Err(HostError::Failed);
            }
            self.revise_count += 1;
            self.last_revise_capability = capability;
            let mut result = HostCallResult::empty(0);
            result.object_id = object_id;
            result.revision = 2;
            Ok(result)
        }

        fn object_history(
            &mut self,
            _capability: PackedCapability,
            object_id: u64,
        ) -> Result<HostCallResult, HostError> {
            if object_id != 1042 {
                return Err(HostError::Failed);
            }
            let mut result = HostCallResult::empty(0);
            result.object_id = object_id;
            result.revision = 2;
            Ok(result)
        }

        fn task_context(
            &mut self,
            _capability: PackedCapability,
        ) -> Result<HostCallResult, HostError> {
            let mut result = HostCallResult::empty(0);
            result.object_id = 0x1001;
            result.revision = 0x2001;
            result.capability = PackedCapability::from_raw(85);
            result.reserved0 = u32::from(TaskProposalKind::Related.code());
            result.bytes_len = 12;
            result.bytes[..12].copy_from_slice(b"context-move");
            Ok(result)
        }

        fn task_propose(
            &mut self,
            _capability: PackedCapability,
            candidate_task_id: u64,
            score: u64,
        ) -> Result<(), HostError> {
            self.proposal_count += 1;
            self.last_candidate_task_id = candidate_task_id;
            self.last_score = score;
            Ok(())
        }

        fn command_read(
            &mut self,
            _capability: PackedCapability,
        ) -> Result<HostCallResult, HostError> {
            self.create_count += 1;
            let mut result = HostCallResult::empty(0);
            result.reserved0 = u32::from(COMMAND_KIND_CREATE_NOTE);
            result.bytes_len = 16;
            result.bytes[..16].copy_from_slice(b"command-accepted");
            Ok(result)
        }

        fn command_result_emit(
            &mut self,
            _capability: PackedCapability,
            status: u16,
            text: &[u8],
        ) -> Result<(), HostError> {
            self.proposal_count += 1;
            self.last_score = u64::from(status);
            self.last_text[..text.len()].copy_from_slice(text);
            self.last_text_len = text.len();
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
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(7, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 64, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(exit.executed_nodes, 5);
        assert_eq!(exit.result_type, GRAPH_RESULT_UNIT);
        assert_eq!(host.log_count, 1);
        assert_eq!(&host.logs[0][..5], b"hello");
    }

    #[test]
    fn self_jump_graph_exits_on_instruction_budget() {
        let bytes = test_support::self_jump_budget_loop();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(7, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 3, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_BUDGET_EXHAUSTED);
        assert_eq!(exit.error_code, RuntimeError::BudgetExhausted.code());
        assert_eq!(exit.executed_nodes, 3);
        assert_eq!(exit.last_node, 1);
        assert_eq!(host.log_count, 0);
    }

    #[test]
    fn branch_terminator_selects_boolean_target() {
        for (condition, expected_last_node) in [(true, 2), (false, 3)] {
            let bytes = test_support::branch_to_return_package(condition);
            let package = PythGraphPackage::decode(&bytes).unwrap();
            let verified = verify_package(&package).unwrap();
            let mut host = RecordingHost {
                logs: [[0; 16]; 4],
                log_count: 0,
                create_count: 0,
                revise_count: 0,
                inspect_count: 0,
                last_revise_capability: PackedCapability::from_raw(0),
                last_inspect_capability: PackedCapability::from_raw(0),
                last_text: [0; 16],
                last_text_len: 0,
                malformed_create: false,
                deny_create: false,
                proposal_count: 0,
                last_candidate_task_id: 0,
                last_score: 0,
            };
            let imports = [PackedCapability::from_parts(7, 1); MAX_PYTH_GRAPH_IMPORTS];
            let mut values = [None; MAX_RUNTIME_VALUES];
            let mut host_results = [None; MAX_RUNTIME_VALUES];

            let exit = Interpreter::new(verified, &imports, 16, &mut values, &mut host_results)
                .execute(&mut host);

            assert_eq!(exit.status, GRAPH_EXIT_OK);
            assert_eq!(exit.executed_nodes, 3);
            assert_eq!(exit.last_node, expected_last_node);
        }
    }

    #[test]
    fn object_create_revise_and_inspect_propagate_dynamic_capability() {
        let bytes = test_support::object_note_flow_package();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = PackedCapability::from_parts(4, 1);
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(
            values[9],
            Some(Value::HostUtf8 {
                producer_node: 8,
                len: 5
            })
        );
        assert_eq!(host.create_count, 1);
        assert_eq!(host.revise_count, 1);
        assert_eq!(host.inspect_count, 1);
        assert_eq!(
            host.last_revise_capability,
            PackedCapability::from_parts(9, 2)
        );
        assert_eq!(
            host.last_inspect_capability,
            PackedCapability::from_parts(9, 2)
        );
        assert_eq!(&host.last_text[..host.last_text_len], b"hello");
    }

    #[test]
    fn known_object_denial_status_executes_through_object_id_constant() {
        let bytes = test_support::object_known_denied_package();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = PackedCapability::from_parts(4, 1);
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(values[7], Some(Value::ErrorCode(1)));
        assert_eq!(host.inspect_count, 1);
        assert_eq!(
            host.last_inspect_capability,
            PackedCapability::from_parts(9, 2)
        );
    }

    #[test]
    fn restore_query_inspect_history_fixture_executes_with_rebound_capability() {
        let bytes = test_support::object_restore_package();
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
        imports[0] = PackedCapability::from_parts(4, 1);
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(
            values[7],
            Some(Value::HostUtf8 {
                producer_node: 6,
                len: 5
            })
        );
        assert_eq!(host_results[8].unwrap().revision, 2);
        assert_eq!(host.inspect_count, 1);
        assert_eq!(
            host.last_inspect_capability,
            PackedCapability::from_parts(9, 2)
        );
    }

    #[test]
    fn malformed_host_result_metadata_stops_execution() {
        let bytes =
            test_support::object_create_host_result(PythType::ErrorCode, HOST_RESULT_STATUS);
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: true,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(4, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_RUNTIME_ERROR);
    }

    #[test]
    fn denied_object_status_is_available_as_typed_host_result() {
        let bytes =
            test_support::object_create_host_result(PythType::ErrorCode, HOST_RESULT_STATUS);
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: true,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(4, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(host.create_count, 1);
        assert_eq!(values[4], Some(Value::ErrorCode(1)));
    }

    #[test]
    fn task_proposal_emit_invokes_proposal_only_host() {
        let bytes = test_support::task_proposal_emit_with_import_rights(RIGHTS_CREATE);
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(11, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(host.proposal_count, 1);
        assert_eq!(host.last_candidate_task_id, 0x2001);
        assert_eq!(host.last_score, 85);
    }

    #[test]
    fn task_context_read_exposes_confidence_score_as_typed_host_result() {
        let bytes = test_support::task_context_score_with_import_rights(RIGHTS_READ);
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(11, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(values[3], Some(Value::U64(85)));
    }

    #[test]
    fn command_read_and_result_emit_execute_through_typed_host_surface() {
        let bytes = test_support::command_read_result_emit_with_import_rights(
            RIGHTS_READ | pythos_shared::pyth_tig::opcode::RIGHTS_APPEND,
        );
        let package = PythGraphPackage::decode(&bytes).unwrap();
        let verified = verify_package(&package).unwrap();
        let mut host = RecordingHost {
            logs: [[0; 16]; 4],
            log_count: 0,
            create_count: 0,
            revise_count: 0,
            inspect_count: 0,
            last_revise_capability: PackedCapability::from_raw(0),
            last_inspect_capability: PackedCapability::from_raw(0),
            last_text: [0; 16],
            last_text_len: 0,
            malformed_create: false,
            deny_create: false,
            proposal_count: 0,
            last_candidate_task_id: 0,
            last_score: 0,
        };
        let imports = [PackedCapability::from_parts(12, 1); MAX_PYTH_GRAPH_IMPORTS];
        let mut values = [None; MAX_RUNTIME_VALUES];
        let mut host_results = [None; MAX_RUNTIME_VALUES];

        let exit = Interpreter::new(verified, &imports, 128, &mut values, &mut host_results)
            .execute(&mut host);

        assert_eq!(exit.status, GRAPH_EXIT_OK);
        assert_eq!(host.create_count, 1);
        assert_eq!(host.proposal_count, 1);
        assert_eq!(host.last_score, 0);
        assert_eq!(&host.last_text[..host.last_text_len], b"command-accepted");
        assert_eq!(values[3], Some(Value::U64(3)));
    }
}
