use crate::{
    ast::{Expression, Ident, Literal, Program, Statement},
    intrinsics::{HostProducer, HostResultAccess, Intrinsic},
    parser::parse_source,
    span::{Diagnostic, Span},
    types::{PythType, ast_type_to_pyth_type, is_integer_like_type},
};
use pythos_shared::pyth_tig::opcode::{
    RESOURCE_COMMAND, RESOURCE_GRAPH, RESOURCE_OBJECT, RESOURCE_OBJECT_WORKSPACE,
    RESOURCE_SYSTEM_LOG, RESOURCE_TASK, RIGHTS_APPEND, RIGHTS_APPROVE, RIGHTS_CONTROL,
    RIGHTS_CREATE, RIGHTS_QUERY, RIGHTS_READ, RIGHTS_REVISE,
};

const MAX_LOOP_BUDGET: u64 = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedProgram {
    pub main: TypedFunction,
    pub required_intrinsics: Vec<Intrinsic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypedFunction {
    pub result_type: PythType,
}

pub fn typecheck_source(source: &str) -> Result<TypedProgram, Diagnostic> {
    let program = parse_source(source)?;
    typecheck_program(&program)
}

pub fn typecheck_program(program: &Program) -> Result<TypedProgram, Diagnostic> {
    let mut checker = Checker::new();
    checker.check_program(program)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityInfo {
    resource_kind: u16,
    rights: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SymbolInfo {
    ty: PythType,
    capability: Option<CapabilityInfo>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExprOutcome {
    ty: PythType,
    capability: Option<CapabilityInfo>,
    producer: Option<HostProducer>,
    host_result_access: bool,
}

struct Checker {
    symbols: Vec<(String, SymbolInfo)>,
    required_intrinsics: Vec<Intrinsic>,
    pending_host_producer: Option<HostProducer>,
}

impl Checker {
    fn new() -> Self {
        Self {
            symbols: Vec::new(),
            required_intrinsics: Vec::new(),
            pending_host_producer: None,
        }
    }

    fn check_program(&mut self, program: &Program) -> Result<TypedProgram, Diagnostic> {
        for import in &program.imports {
            let capability = capability_info_for_import(&import.resource, &import.rights)
                .map_err(|(code, message)| Diagnostic::new(code, message, import.span))?;
            self.define_symbol(
                &import.name,
                SymbolInfo {
                    ty: PythType::Capability,
                    capability: Some(capability),
                },
            )?;
        }

        for statement in &program.main.statements {
            self.check_statement(statement)?;
        }

        Ok(TypedProgram {
            main: TypedFunction {
                result_type: PythType::Unit,
            },
            required_intrinsics: self.required_intrinsics.clone(),
        })
    }

    fn check_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Let {
                name, ty, value, ..
            } => {
                let expected = ast_type_to_pyth_type(*ty);
                let outcome = self.check_expression(value, Some(expected))?;
                if outcome.ty != expected {
                    return Err(Diagnostic::new("T0003", "type mismatch", value.span()));
                }
                self.define_symbol(
                    name,
                    SymbolInfo {
                        ty: expected,
                        capability: (expected == PythType::Capability)
                            .then_some(outcome.capability)
                            .flatten(),
                    },
                )?;
                self.finish_expression(outcome);
            }
            Statement::If {
                condition,
                then_statements,
                else_statements,
                ..
            } => {
                self.check_condition(condition)?;
                self.check_nested_block(then_statements)?;
                self.check_nested_block(else_statements)?;
                self.pending_host_producer = None;
            }
            Statement::While {
                budget,
                condition,
                statements,
                span,
            } => {
                if *budget > MAX_LOOP_BUDGET {
                    return Err(Diagnostic::new("T0014", "loop budget exceeds 65536", *span));
                }
                self.check_condition(condition)?;
                self.check_nested_block(statements)?;
                self.pending_host_producer = None;
            }
            Statement::Expr(expr) => {
                let outcome = self.check_expression(expr, None)?;
                self.finish_expression(outcome);
            }
            Statement::Return { .. } => {
                self.pending_host_producer = None;
            }
        }

        Ok(())
    }

    fn check_nested_block(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        let symbol_len = self.symbols.len();
        let saved_pending = self.pending_host_producer;
        self.pending_host_producer = None;
        for statement in statements {
            self.check_statement(statement)?;
        }
        self.symbols.truncate(symbol_len);
        self.pending_host_producer = saved_pending;
        Ok(())
    }

    fn check_condition(&mut self, condition: &Expression) -> Result<(), Diagnostic> {
        let outcome = self.check_expression(condition, Some(PythType::Bool))?;
        if outcome.ty != PythType::Bool {
            return Err(Diagnostic::new(
                "T0004",
                "non-bool condition",
                condition.span(),
            ));
        }
        self.pending_host_producer = None;
        Ok(())
    }

    fn check_expression(
        &mut self,
        expr: &Expression,
        expected: Option<PythType>,
    ) -> Result<ExprOutcome, Diagnostic> {
        match expr {
            Expression::Literal(literal) => self.check_literal(literal, expected),
            Expression::Name(name) => self
                .lookup_symbol(&name.text)
                .map(|symbol| ExprOutcome {
                    ty: symbol.ty,
                    capability: symbol.capability,
                    producer: None,
                    host_result_access: false,
                })
                .ok_or_else(|| Diagnostic::new("T0002", "unknown name", name.span)),
            Expression::Call { callee, args, span } => self.check_call(callee, args, *span),
            Expression::Unary { expr, span, .. } => {
                let operand = self.check_expression(expr, Some(PythType::Bool))?;
                if operand.ty == PythType::Capability {
                    return Err(Diagnostic::new(
                        "T0008",
                        "capability operation forbidden",
                        *span,
                    ));
                }
                if operand.ty != PythType::Bool {
                    return Err(Diagnostic::new("T0003", "type mismatch", expr.span()));
                }
                Ok(ExprOutcome::plain(PythType::Bool))
            }
            Expression::Binary {
                op,
                left,
                right,
                span,
            } => {
                let left = self.check_expression(left, None)?;
                let right = self.check_expression(right, None)?;
                if left.ty == PythType::Capability || right.ty == PythType::Capability {
                    return Err(Diagnostic::new(
                        "T0008",
                        "capability operation forbidden",
                        *span,
                    ));
                }
                let ty = match op {
                    crate::ast::BinaryOp::Equal => {
                        if left.ty != right.ty {
                            return Err(Diagnostic::new("T0003", "type mismatch", *span));
                        }
                        PythType::Bool
                    }
                    crate::ast::BinaryOp::Less
                    | crate::ast::BinaryOp::Add
                    | crate::ast::BinaryOp::Subtract => {
                        if left.ty != PythType::U64 || right.ty != PythType::U64 {
                            return Err(Diagnostic::new("T0003", "type mismatch", *span));
                        }
                        if matches!(op, crate::ast::BinaryOp::Less) {
                            PythType::Bool
                        } else {
                            PythType::U64
                        }
                    }
                    crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or => {
                        if left.ty != PythType::Bool || right.ty != PythType::Bool {
                            return Err(Diagnostic::new("T0003", "type mismatch", *span));
                        }
                        PythType::Bool
                    }
                };
                Ok(ExprOutcome::plain(ty))
            }
        }
    }

    fn check_literal(
        &self,
        literal: &Literal,
        expected: Option<PythType>,
    ) -> Result<ExprOutcome, Diagnostic> {
        match literal {
            Literal::Bool { .. } => Ok(ExprOutcome::plain(PythType::Bool)),
            Literal::String { .. } => Ok(ExprOutcome::plain(PythType::Utf8)),
            Literal::Integer { text, span } => {
                if text.parse::<u64>().is_err() {
                    return Err(Diagnostic::new("T0013", "integer literal overflow", *span));
                }
                let ty = expected
                    .filter(|ty| is_integer_like_type(*ty))
                    .unwrap_or(PythType::U64);
                Ok(ExprOutcome::plain(ty))
            }
        }
    }

    fn check_call(
        &mut self,
        callee: &Ident,
        args: &[Expression],
        span: Span,
    ) -> Result<ExprOutcome, Diagnostic> {
        let intrinsic = Intrinsic::from_name(&callee.text)
            .ok_or_else(|| Diagnostic::new("T0006", "unsupported intrinsic", callee.span))?;
        self.record_intrinsic(intrinsic);

        let expected_args = intrinsic.arg_types();
        if args.len() != expected_args.len() {
            return Err(Diagnostic::new("T0007", "wrong argument count", span));
        }

        let mut first_capability = None;
        for (index, (arg, expected)) in args.iter().zip(expected_args.iter().copied()).enumerate() {
            let outcome = self.check_expression(arg, Some(expected))?;
            if outcome.ty != expected {
                return Err(Diagnostic::new("T0003", "type mismatch", arg.span()));
            }
            if index == 0 && expected == PythType::Capability {
                first_capability = outcome.capability;
            }
        }

        if let Some(requirement) = intrinsic.requirement() {
            let Some(capability) = first_capability else {
                return Err(Diagnostic::new("T0011", "import resource mismatch", span));
            };
            if capability.resource_kind != requirement.resource_kind {
                return Err(Diagnostic::new("T0011", "import resource mismatch", span));
            }
            if capability.rights & requirement.rights != requirement.rights {
                return Err(Diagnostic::new("T0012", "import rights insufficient", span));
            }
        }

        if let Some(access) = intrinsic.host_result_access() {
            self.validate_host_result_access(access, span)?;
        }

        Ok(ExprOutcome {
            ty: intrinsic.result_type(),
            capability: capability_for_intrinsic_result(intrinsic),
            producer: intrinsic.producer(),
            host_result_access: intrinsic.host_result_access().is_some(),
        })
    }

    fn validate_host_result_access(
        &self,
        access: HostResultAccess,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let valid = matches!(
            (self.pending_host_producer, access),
            (
                Some(HostProducer::ObjectCreate),
                HostResultAccess::CreatedCapability | HostResultAccess::CreatedRevision
            ) | (
                Some(HostProducer::ObjectQuery),
                HostResultAccess::QueriedCapability
            ) | (
                Some(HostProducer::ObjectInspect),
                HostResultAccess::InspectedRevision
            )
        );
        if valid {
            Ok(())
        } else {
            Err(Diagnostic::new("T0010", "stale host result access", span))
        }
    }

    fn finish_expression(&mut self, outcome: ExprOutcome) {
        if !outcome.host_result_access {
            self.pending_host_producer = outcome.producer;
        }
    }

    fn record_intrinsic(&mut self, intrinsic: Intrinsic) {
        if !self.required_intrinsics.contains(&intrinsic) {
            self.required_intrinsics.push(intrinsic);
        }
    }

    fn define_symbol(&mut self, ident: &Ident, info: SymbolInfo) -> Result<(), Diagnostic> {
        if self.symbols.iter().any(|(name, _)| name == &ident.text) {
            return Err(Diagnostic::new("T0001", "duplicate local", ident.span));
        }
        self.symbols.push((ident.text.clone(), info));
        Ok(())
    }

    fn lookup_symbol(&self, name: &str) -> Option<SymbolInfo> {
        self.symbols
            .iter()
            .rev()
            .find_map(|(candidate, info)| (candidate == name).then_some(*info))
    }
}

impl ExprOutcome {
    const fn plain(ty: PythType) -> Self {
        Self {
            ty,
            capability: None,
            producer: None,
            host_result_access: false,
        }
    }
}

fn capability_info_for_import(
    resource: &str,
    rights: &str,
) -> Result<CapabilityInfo, (&'static str, &'static str)> {
    let resource_kind = resource_kind(resource).ok_or(("T0011", "import resource mismatch"))?;
    let rights = rights_bits(rights).ok_or(("T0012", "import rights insufficient"))?;
    Ok(CapabilityInfo {
        resource_kind,
        rights,
    })
}

fn resource_kind(resource: &str) -> Option<u16> {
    Some(match resource {
        "system.log" => RESOURCE_SYSTEM_LOG,
        "object.workspace" => RESOURCE_OBJECT_WORKSPACE,
        "object" => RESOURCE_OBJECT,
        "task" => RESOURCE_TASK,
        "graph" => RESOURCE_GRAPH,
        "command" => RESOURCE_COMMAND,
        _ => return None,
    })
}

fn rights_bits(rights: &str) -> Option<u64> {
    let mut bits = 0u64;
    for right in rights.split('|') {
        bits |= match right {
            "read" | "write" => RIGHTS_READ,
            "query" => RIGHTS_QUERY,
            "revise" => RIGHTS_REVISE,
            "create" => RIGHTS_CREATE,
            "append" => RIGHTS_APPEND,
            "approve" => RIGHTS_APPROVE,
            "control" => RIGHTS_CONTROL,
            _ => return None,
        };
    }
    Some(bits)
}

fn capability_for_intrinsic_result(intrinsic: Intrinsic) -> Option<CapabilityInfo> {
    match intrinsic {
        Intrinsic::ObjectCreatedCapability | Intrinsic::ObjectQueriedCapability => {
            Some(CapabilityInfo {
                resource_kind: RESOURCE_OBJECT,
                rights: RIGHTS_READ | RIGHTS_REVISE,
            })
        }
        _ => None,
    }
}
