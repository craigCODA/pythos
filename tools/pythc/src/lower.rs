use crate::{
    ast::{BinaryOp, Expression, Literal, Program, Statement, UnaryOp},
    graph::{GraphBlock, GraphImport, GraphNode, OwnedGraph},
    intrinsics::{HostProducer, HostResultAccess, Intrinsic},
    span::{Diagnostic, Span},
    typecheck::TypedProgram,
    types::{PythType, is_integer_like_type},
};
use pythos_shared::{
    pyth_runtime_abi::{
        HOST_RESULT_CAPABILITY, HOST_RESULT_OBJECT_ID, HOST_RESULT_REVISION, HOST_RESULT_UTF8,
    },
    pyth_tig::{
        NO_VALUE,
        opcode::{
            Opcode, RESOURCE_COMMAND, RESOURCE_GRAPH, RESOURCE_OBJECT, RESOURCE_OBJECT_WORKSPACE,
            RESOURCE_SYSTEM_LOG, RESOURCE_TASK, RIGHTS_APPEND, RIGHTS_APPROVE, RIGHTS_CONTROL,
            RIGHTS_CREATE, RIGHTS_QUERY, RIGHTS_READ, RIGHTS_REVISE,
        },
    },
    task_abi::{
        TASK_CONTEXT_RESULT_ACTIVE_TASK_ID, TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID,
        TASK_CONTEXT_RESULT_CONFIDENCE_SCORE, TASK_CONTEXT_RESULT_PROPOSAL_KIND,
        TASK_CONTEXT_RESULT_REASON_UTF8,
    },
};

pub fn lower_program(typed: &TypedProgram) -> Result<OwnedGraph, Diagnostic> {
    Lowerer::new(&typed.program).lower()
}

struct Lowerer<'a> {
    program: &'a Program,
    builder: GraphBuilder,
    pending_host_producer: Option<(HostProducer, u32)>,
    pending_task_context_producer: Option<(u32, u32)>,
}

impl<'a> Lowerer<'a> {
    fn new(program: &'a Program) -> Self {
        Self {
            program,
            builder: GraphBuilder::new(program.principal_id, &program.name.text),
            pending_host_producer: None,
            pending_task_context_producer: None,
        }
    }

    fn lower(mut self) -> Result<OwnedGraph, Diagnostic> {
        self.builder.start_entry();
        self.lower_imports()?;

        if let Some(budget) = first_loop_budget(&self.program.main.statements) {
            self.builder.loop_budgets.push(budget);
            self.builder.push_jump(self.builder.current_block as u32);
            return self.builder.finish();
        }

        self.lower_statements(&self.program.main.statements)?;
        if !self.builder.current_block_is_terminated() {
            self.builder.push_return();
        }
        self.builder.finish()
    }

    fn lower_imports(&mut self) -> Result<(), Diagnostic> {
        for (slot, import) in self.program.imports.iter().enumerate() {
            let (resource_kind, rights) = import_contract(&import.resource, &import.rights)
                .map_err(|(code, message)| Diagnostic::new(code, message, import.span))?;
            let import_slot = u16::try_from(slot).map_err(|_| {
                Diagnostic::new(
                    "G0001",
                    "shared verifier rejected compiler output",
                    import.span,
                )
            })?;
            self.builder
                .add_import(&import.name.text, resource_kind, rights, import_slot);
            let node = self.builder.push_node(
                Opcode::BlockParam,
                PythType::Capability,
                [NO_VALUE; 4],
                u32::from(import_slot),
                0,
                0,
            );
            self.builder.define_local(import.name.text.clone(), node);
        }
        Ok(())
    }

    fn lower_statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            if self.builder.current_block_is_terminated() {
                break;
            }
            match statement {
                Statement::Let {
                    name, ty, value, ..
                } => {
                    let expected = crate::types::ast_type_to_pyth_type(*ty);
                    let value = self.lower_expression(value, Some(expected))?;
                    self.builder.define_local(name.text.clone(), value);
                }
                Statement::Expr(expr) => {
                    self.lower_expression(expr, None)?;
                }
                Statement::Return { .. } => {
                    self.builder.push_return();
                }
                Statement::If {
                    condition,
                    then_statements,
                    else_statements,
                    ..
                } => {
                    self.lower_if(condition, then_statements, else_statements)?;
                }
                Statement::While { budget, .. } => {
                    self.builder.loop_budgets.push(*budget);
                    self.builder.push_jump(self.builder.current_block as u32);
                }
            }
        }
        Ok(())
    }

    fn lower_if(
        &mut self,
        condition: &Expression,
        then_statements: &[Statement],
        else_statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        let condition = self.lower_expression(condition, Some(PythType::Bool))?;
        let then_block = self.builder.add_block();
        let else_block = self.builder.add_block();
        let join_block = self.builder.add_block();
        self.builder
            .push_branch(condition, then_block as u32, else_block as u32);

        self.builder.switch_block(then_block);
        self.lower_statements(then_statements)?;
        if !self.builder.current_block_is_terminated() {
            self.builder.push_jump(join_block as u32);
        }

        self.builder.switch_block(else_block);
        self.lower_statements(else_statements)?;
        if !self.builder.current_block_is_terminated() {
            self.builder.push_jump(join_block as u32);
        }

        self.builder.switch_block(join_block);
        Ok(())
    }

    fn lower_expression(
        &mut self,
        expr: &Expression,
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        match expr {
            Expression::Literal(literal) => self.lower_literal(literal, expected),
            Expression::Name(name) => self.builder.lookup_local(&name.text).ok_or_else(|| {
                Diagnostic::new(
                    "G0001",
                    "shared verifier rejected compiler output",
                    name.span,
                )
            }),
            Expression::Call { callee, args, span } => {
                let intrinsic = Intrinsic::from_name(&callee.text).ok_or_else(|| {
                    Diagnostic::new(
                        "G0001",
                        "shared verifier rejected compiler output",
                        callee.span,
                    )
                })?;
                self.lower_intrinsic(intrinsic, args, *span, expected)
            }
            Expression::Unary { op, expr, .. } => {
                let input = self.lower_expression(expr, Some(PythType::Bool))?;
                let opcode = match op {
                    UnaryOp::Not => Opcode::BoolNot,
                };
                Ok(self.builder.push_node(
                    opcode,
                    PythType::Bool,
                    [input, NO_VALUE, NO_VALUE, NO_VALUE],
                    0,
                    0,
                    0,
                ))
            }
            Expression::Binary {
                op,
                left,
                right,
                span: _,
            } => {
                let (opcode, result, expected_inputs) = match op {
                    BinaryOp::Equal => (Opcode::Eq, PythType::Bool, PythType::U64),
                    BinaryOp::Less => (Opcode::LessThanU64, PythType::Bool, PythType::U64),
                    BinaryOp::Add => (Opcode::AddU64, PythType::U64, PythType::U64),
                    BinaryOp::Subtract => (Opcode::SubU64, PythType::U64, PythType::U64),
                    BinaryOp::And => (Opcode::BoolAnd, PythType::Bool, PythType::Bool),
                    BinaryOp::Or => (Opcode::BoolOr, PythType::Bool, PythType::Bool),
                };
                let left = self.lower_expression(left, Some(expected_inputs))?;
                let right = self.lower_expression(right, Some(expected_inputs))?;
                Ok(self.builder.push_node(
                    opcode,
                    result,
                    [left, right, NO_VALUE, NO_VALUE],
                    0,
                    0,
                    0,
                ))
            }
        }
    }

    fn lower_literal(
        &mut self,
        literal: &Literal,
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        match literal {
            Literal::Bool { value, .. } => Ok(self.builder.push_node(
                Opcode::ConstBool,
                PythType::Bool,
                [NO_VALUE; 4],
                0,
                0,
                u64::from(*value),
            )),
            Literal::String { value, .. } => {
                let bytes = value.as_bytes();
                let (opcode, result_type, offset, len) = if expected == Some(PythType::Bytes) {
                    let (offset, len) = self.builder.intern_constant(bytes)?;
                    (Opcode::ConstBytes, PythType::Bytes, offset, len)
                } else {
                    let (offset, len) = self.builder.intern_string(bytes)?;
                    (Opcode::ConstUtf8, PythType::Utf8, offset, len)
                };
                Ok(self.builder.push_node(
                    opcode,
                    result_type,
                    [NO_VALUE; 4],
                    offset,
                    u32::from(len),
                    0,
                ))
            }
            Literal::Integer { text, span } => {
                let value = text.parse::<u64>().map_err(|_| {
                    Diagnostic::new("G0001", "shared verifier rejected compiler output", *span)
                })?;
                let ty = expected
                    .filter(|ty| is_integer_like_type(*ty))
                    .unwrap_or(PythType::U64);
                let opcode = if ty == PythType::I64 {
                    Opcode::ConstI64
                } else {
                    Opcode::ConstU64
                };
                Ok(self
                    .builder
                    .push_node(opcode, ty, [NO_VALUE; 4], 0, 0, value))
            }
        }
    }

    fn lower_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        args: &[Expression],
        span: Span,
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        match intrinsic {
            Intrinsic::ObjectCreatedCapability
            | Intrinsic::ObjectCreatedRevision
            | Intrinsic::ObjectQueriedCapability
            | Intrinsic::ObjectInspectedRevision => {
                let access = intrinsic
                    .host_result_access()
                    .ok_or_else(|| compiler_rejected(span))?;
                self.lower_host_result_access(access, span)
            }
            Intrinsic::ObjectCreate | Intrinsic::ObjectQuery => {
                self.lower_object_workspace_intrinsic(intrinsic, args, span, expected)
            }
            Intrinsic::ObjectInspect => {
                self.lower_object_read_intrinsic(intrinsic, args, span, expected)
            }
            Intrinsic::ObjectHistory => {
                self.lower_object_read_intrinsic(intrinsic, args, span, expected)
            }
            Intrinsic::ObjectRevise => self.lower_object_revise(args, expected),
            Intrinsic::TaskContextActive
            | Intrinsic::TaskContextCandidate
            | Intrinsic::TaskContextScore
            | Intrinsic::TaskContextKind
            | Intrinsic::TaskContextReason => {
                self.lower_task_context_accessor(intrinsic, args, span)
            }
            Intrinsic::TaskPropose => self.lower_task_propose(args, expected, span),
            _ => self.lower_generic_effectful_intrinsic(intrinsic, args, span),
        }
    }

    fn lower_generic_effectful_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        args: &[Expression],
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let opcode = opcode_for_intrinsic(intrinsic).ok_or_else(|| compiler_rejected(span))?;
        let arg_types = intrinsic.arg_types();
        let mut lowered_args = [NO_VALUE; 4];
        lowered_args[0] = self.builder.current_effect;
        for (index, (arg, expected)) in args.iter().zip(arg_types.iter().copied()).enumerate() {
            lowered_args[index + 1] = self.lower_expression(arg, Some(expected))?;
        }
        Ok(self.push_effect_node(opcode, lowered_args, None))
    }

    fn lower_object_workspace_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        args: &[Expression],
        span: Span,
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        let opcode = opcode_for_intrinsic(intrinsic).ok_or_else(|| compiler_rejected(span))?;
        let capability = self.lower_expression(&args[0], Some(PythType::Capability))?;
        let object_kind = self.lower_object_kind(&args[1])?;
        let producer = self.push_effect_node(
            opcode,
            [
                self.builder.current_effect,
                capability,
                object_kind,
                NO_VALUE,
            ],
            intrinsic.producer(),
        );
        if expected.is_some() {
            Ok(self.push_host_result(producer, HOST_RESULT_OBJECT_ID, PythType::ObjectId))
        } else {
            Ok(producer)
        }
    }

    fn lower_object_revise(
        &mut self,
        args: &[Expression],
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        let capability = self.lower_expression(&args[0], Some(PythType::Capability))?;
        let object_id = self.lower_expression(&args[1], Some(PythType::ObjectId))?;
        let payload = self.lower_revision_payload(&args[3])?;
        let producer = self.push_effect_node(
            Opcode::ObjectRevise,
            [self.builder.current_effect, capability, object_id, payload],
            None,
        );
        if expected.is_some() {
            Ok(self.push_host_result(producer, HOST_RESULT_REVISION, PythType::RevisionId))
        } else {
            Ok(producer)
        }
    }

    fn lower_object_read_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        args: &[Expression],
        span: Span,
        expected: Option<PythType>,
    ) -> Result<u32, Diagnostic> {
        let opcode = opcode_for_intrinsic(intrinsic).ok_or_else(|| compiler_rejected(span))?;
        let capability = self.lower_expression(&args[0], Some(PythType::Capability))?;
        let object_id = self.lower_expression(&args[1], Some(PythType::ObjectId))?;
        let producer = self.push_effect_node(
            opcode,
            [self.builder.current_effect, capability, object_id, NO_VALUE],
            intrinsic.producer(),
        );
        match (intrinsic, expected) {
            (Intrinsic::ObjectInspect, Some(PythType::Utf8)) => {
                Ok(self.push_host_result(producer, HOST_RESULT_UTF8, PythType::Utf8))
            }
            (Intrinsic::ObjectHistory, Some(_)) => Err(compiler_rejected(span)),
            _ => Ok(producer),
        }
    }

    fn lower_host_result_access(
        &mut self,
        access: HostResultAccess,
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let Some((producer, node)) = self.pending_host_producer else {
            return Err(compiler_rejected(span));
        };
        let (field, result_type) = match (producer, access) {
            (HostProducer::ObjectCreate, HostResultAccess::CreatedCapability)
            | (HostProducer::ObjectQuery, HostResultAccess::QueriedCapability) => {
                (HOST_RESULT_CAPABILITY, PythType::Capability)
            }
            (HostProducer::ObjectCreate, HostResultAccess::CreatedRevision)
            | (HostProducer::ObjectInspect, HostResultAccess::InspectedRevision) => {
                (HOST_RESULT_REVISION, PythType::RevisionId)
            }
            _ => return Err(compiler_rejected(span)),
        };
        Ok(self.push_host_result(node, field, result_type))
    }

    fn lower_task_context_accessor(
        &mut self,
        intrinsic: Intrinsic,
        args: &[Expression],
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let capability = self.lower_expression(&args[0], Some(PythType::Capability))?;
        let producer = match self.pending_task_context_producer {
            Some((pending_capability, producer))
                if pending_capability == capability && self.builder.current_effect == producer =>
            {
                producer
            }
            _ => self.push_effect_node(
                Opcode::TaskContextRead,
                [self.builder.current_effect, capability, NO_VALUE, NO_VALUE],
                None,
            ),
        };
        self.pending_task_context_producer = Some((capability, producer));
        let (field, result_type) = match intrinsic {
            Intrinsic::TaskContextActive => (TASK_CONTEXT_RESULT_ACTIVE_TASK_ID, PythType::TaskId),
            Intrinsic::TaskContextCandidate => {
                (TASK_CONTEXT_RESULT_CANDIDATE_TASK_ID, PythType::TaskId)
            }
            Intrinsic::TaskContextScore => (TASK_CONTEXT_RESULT_CONFIDENCE_SCORE, PythType::U64),
            Intrinsic::TaskContextKind => (TASK_CONTEXT_RESULT_PROPOSAL_KIND, PythType::U64),
            Intrinsic::TaskContextReason => (TASK_CONTEXT_RESULT_REASON_UTF8, PythType::Utf8),
            _ => return Err(compiler_rejected(span)),
        };
        Ok(self.push_host_result(producer, field, result_type))
    }

    fn lower_task_propose(
        &mut self,
        args: &[Expression],
        expected: Option<PythType>,
        span: Span,
    ) -> Result<u32, Diagnostic> {
        if expected.is_some() {
            return Err(compiler_rejected(span));
        }
        let capability = self.lower_expression(&args[0], Some(PythType::Capability))?;
        let candidate = self.lower_expression(&args[1], Some(PythType::TaskId))?;
        let score = self.lower_expression(&args[2], Some(PythType::U64))?;
        Ok(self.push_effect_node(
            Opcode::TaskProposalEmit,
            [self.builder.current_effect, capability, candidate, score],
            None,
        ))
    }

    fn lower_object_kind(&mut self, expr: &Expression) -> Result<u32, Diagnostic> {
        let Expression::Literal(Literal::Integer { text, span }) = expr else {
            return Err(compiler_rejected(expr.span()));
        };
        let kind = match text.parse::<u64>() {
            Ok(1) => b"note".as_slice(),
            _ => return Err(compiler_rejected(*span)),
        };
        let (offset, len) = self.builder.intern_string(kind)?;
        Ok(self.builder.push_node(
            Opcode::ConstUtf8,
            PythType::Utf8,
            [NO_VALUE; 4],
            offset,
            u32::from(len),
            0,
        ))
    }

    fn lower_revision_payload(&mut self, expr: &Expression) -> Result<u32, Diagnostic> {
        match expr {
            Expression::Literal(Literal::String { .. }) => {
                self.lower_expression(expr, Some(PythType::Bytes))
            }
            _ => Err(compiler_rejected(expr.span())),
        }
    }

    fn push_effect_node(
        &mut self,
        opcode: Opcode,
        inputs: [u32; 4],
        producer: Option<HostProducer>,
    ) -> u32 {
        let effect_node = self
            .builder
            .push_node(opcode, PythType::Effect, inputs, 0, 0, 0);
        self.builder.current_effect = effect_node;
        if opcode != Opcode::TaskContextRead {
            self.pending_task_context_producer = None;
        }
        self.pending_host_producer = producer.map(|producer| (producer, effect_node));
        effect_node
    }

    fn push_host_result(&mut self, producer: u32, field: u32, result_type: PythType) -> u32 {
        self.builder.push_node(
            Opcode::HostResult,
            result_type,
            [producer, NO_VALUE, NO_VALUE, NO_VALUE],
            field,
            0,
            0,
        )
    }
}

struct GraphBuilder {
    principal_id: u64,
    imports: Vec<GraphImport>,
    block_builders: Vec<BlockBuilder>,
    nodes: Vec<GraphNode>,
    constant_pool: Vec<u8>,
    string_table: Vec<u8>,
    loop_budgets: Vec<u64>,
    locals: Vec<(String, u32)>,
    current_block: usize,
    current_effect: u32,
}

impl GraphBuilder {
    fn new(principal_id: u64, program_name: &str) -> Self {
        let mut builder = Self {
            principal_id,
            imports: Vec::new(),
            block_builders: Vec::new(),
            nodes: Vec::new(),
            constant_pool: Vec::new(),
            string_table: Vec::new(),
            loop_budgets: Vec::new(),
            locals: Vec::new(),
            current_block: 0,
            current_effect: NO_VALUE,
        };
        builder
            .string_table
            .extend_from_slice(program_name.as_bytes());
        builder.string_table.push(0);
        builder
    }

    fn start_entry(&mut self) {
        self.add_block();
        let effect = self.push_node(
            Opcode::EffectStart,
            PythType::Effect,
            [NO_VALUE; 4],
            0,
            0,
            0,
        );
        self.current_effect = effect;
    }

    fn add_block(&mut self) -> usize {
        let id = self.block_builders.len();
        self.block_builders.push(BlockBuilder {
            first_node: None,
            node_count: 0,
        });
        id
    }

    fn switch_block(&mut self, block: usize) {
        self.current_block = block;
    }

    fn add_import(&mut self, name: &str, resource_kind: u16, rights: u64, import_slot: u16) {
        let (name_offset, name_len) = self
            .intern_string(name.as_bytes())
            .expect("import names fit the PythTIG string table");
        self.imports.push(GraphImport {
            name_offset,
            name_len,
            resource_kind,
            rights,
            expected_type: PythType::Capability,
            import_slot,
        });
    }

    fn define_local(&mut self, name: String, node: u32) {
        self.locals.push((name, node));
    }

    fn lookup_local(&self, name: &str) -> Option<u32> {
        self.locals
            .iter()
            .rev()
            .find_map(|(candidate, node)| (candidate == name).then_some(*node))
    }

    fn push_branch(&mut self, condition: u32, then_block: u32, else_block: u32) {
        self.push_node(
            Opcode::Branch,
            PythType::Unit,
            [condition, NO_VALUE, NO_VALUE, NO_VALUE],
            then_block,
            else_block,
            0,
        );
    }

    fn push_jump(&mut self, target: u32) {
        self.push_node(Opcode::Jump, PythType::Unit, [NO_VALUE; 4], target, 0, 0);
    }

    fn push_return(&mut self) {
        self.push_node(Opcode::Return, PythType::Unit, [NO_VALUE; 4], 0, 0, 0);
    }

    fn push_node(
        &mut self,
        opcode: Opcode,
        result_type: PythType,
        inputs: [u32; 4],
        auxiliary0: u32,
        auxiliary1: u32,
        immediate: u64,
    ) -> u32 {
        let node_index = self.nodes.len() as u32;
        let block = &mut self.block_builders[self.current_block];
        if block.first_node.is_none() {
            block.first_node = Some(node_index);
        }
        block.node_count += 1;
        self.nodes.push(GraphNode {
            opcode,
            result_type,
            block_index: self.current_block as u16,
            inputs,
            auxiliary0,
            auxiliary1,
            immediate,
        });
        node_index
    }

    fn current_block_is_terminated(&self) -> bool {
        let Some(block) = self.block_builders.get(self.current_block) else {
            return false;
        };
        let Some(first) = block.first_node else {
            return false;
        };
        let last = first + block.node_count - 1;
        self.nodes
            .get(last as usize)
            .is_some_and(|node| node.opcode.signature().terminator)
    }

    fn intern_string(&mut self, bytes: &[u8]) -> Result<(u32, u16), Diagnostic> {
        if let Some(offset) = find_subslice(&self.string_table, bytes) {
            let offset = u32::try_from(offset).map_err(|_| {
                Diagnostic::new(
                    "G0001",
                    "shared verifier rejected compiler output",
                    Span::default(),
                )
            })?;
            let len = u16::try_from(bytes.len()).map_err(|_| {
                Diagnostic::new(
                    "G0001",
                    "shared verifier rejected compiler output",
                    Span::default(),
                )
            })?;
            return Ok((offset, len));
        }
        let offset = u32::try_from(self.string_table.len()).map_err(|_| {
            Diagnostic::new(
                "G0001",
                "shared verifier rejected compiler output",
                Span::default(),
            )
        })?;
        let len = u16::try_from(bytes.len()).map_err(|_| {
            Diagnostic::new(
                "G0001",
                "shared verifier rejected compiler output",
                Span::default(),
            )
        })?;
        self.string_table.extend_from_slice(bytes);
        Ok((offset, len))
    }

    fn intern_constant(&mut self, bytes: &[u8]) -> Result<(u32, u16), Diagnostic> {
        let offset = u32::try_from(self.constant_pool.len()).map_err(|_| {
            Diagnostic::new(
                "G0001",
                "shared verifier rejected compiler output",
                Span::default(),
            )
        })?;
        let len = u16::try_from(bytes.len()).map_err(|_| {
            Diagnostic::new(
                "G0001",
                "shared verifier rejected compiler output",
                Span::default(),
            )
        })?;
        self.constant_pool.extend_from_slice(bytes);
        Ok((offset, len))
    }

    fn finish(self) -> Result<OwnedGraph, Diagnostic> {
        let mut blocks = Vec::new();
        for (id, block) in self.block_builders.into_iter().enumerate() {
            let Some(first_node) = block.first_node else {
                return Err(Diagnostic::new(
                    "G0001",
                    "shared verifier rejected compiler output",
                    Span::default(),
                ));
            };
            let terminator_node = first_node + block.node_count - 1;
            blocks.push(GraphBlock {
                block_id: id as u32,
                first_node,
                node_count: block.node_count,
                parameter_count: 0,
                terminator_node,
            });
        }
        Ok(OwnedGraph {
            principal_id: self.principal_id,
            imports: self.imports,
            blocks,
            nodes: self.nodes,
            constant_pool: self.constant_pool,
            string_table: self.string_table,
            loop_budgets: self.loop_budgets,
        })
    }
}

struct BlockBuilder {
    first_node: Option<u32>,
    node_count: u32,
}

fn first_loop_budget(statements: &[Statement]) -> Option<u64> {
    statements.iter().find_map(|statement| match statement {
        Statement::While { budget, .. } => Some(*budget),
        _ => None,
    })
}

fn opcode_for_intrinsic(intrinsic: Intrinsic) -> Option<Opcode> {
    Some(match intrinsic {
        Intrinsic::SystemLog => Opcode::SystemLog,
        Intrinsic::ObjectCreate => Opcode::ObjectCreate,
        Intrinsic::ObjectQuery => Opcode::ObjectQuery,
        Intrinsic::ObjectInspect => Opcode::ObjectInspect,
        Intrinsic::ObjectRevise => Opcode::ObjectRevise,
        Intrinsic::ObjectHistory => Opcode::ObjectHistory,
        _ => return None,
    })
}

fn compiler_rejected(span: Span) -> Diagnostic {
    Diagnostic::new("G0001", "shared verifier rejected compiler output", span)
}

fn import_contract(
    resource: &str,
    rights: &str,
) -> Result<(u16, u64), (&'static str, &'static str)> {
    let resource_kind = match resource {
        "system.log" => RESOURCE_SYSTEM_LOG,
        "object.workspace" => RESOURCE_OBJECT_WORKSPACE,
        "object" => RESOURCE_OBJECT,
        "task" | "task.context" | "task.proposal" => RESOURCE_TASK,
        "graph" => RESOURCE_GRAPH,
        "command" => RESOURCE_COMMAND,
        _ => return Err(("G0001", "shared verifier rejected compiler output")),
    };
    let mut rights_bits = 0;
    for right in rights.split('|') {
        rights_bits |= match right {
            "read" | "write" => RIGHTS_READ,
            "query" => RIGHTS_QUERY,
            "revise" => RIGHTS_REVISE,
            "create" => RIGHTS_CREATE,
            "append" => RIGHTS_APPEND,
            "approve" => RIGHTS_APPROVE,
            "control" => RIGHTS_CONTROL,
            _ => return Err(("G0001", "shared verifier rejected compiler output")),
        };
    }
    Ok((resource_kind, rights_bits))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
