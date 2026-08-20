use pythos_shared::pyth_tig::{NO_VALUE, opcode::Opcode, types::PythType};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedGraph {
    pub principal_id: u64,
    pub imports: Vec<GraphImport>,
    pub blocks: Vec<GraphBlock>,
    pub nodes: Vec<GraphNode>,
    pub constant_pool: Vec<u8>,
    pub string_table: Vec<u8>,
    pub loop_budgets: Vec<u64>,
}

impl OwnedGraph {
    pub fn contains_opcode(&self, opcode: Opcode) -> bool {
        self.nodes.iter().any(|node| node.opcode == opcode)
    }

    pub fn effect_forks(&self) -> usize {
        let mut consumers = vec![0usize; self.nodes.len()];
        for node in &self.nodes {
            if node.opcode.signature().effectful && node.inputs[0] != NO_VALUE {
                let producer = node.inputs[0] as usize;
                if producer < consumers.len() {
                    consumers[producer] += 1;
                }
            }
        }
        consumers
            .into_iter()
            .filter(|count| *count > 1)
            .map(|count| count - 1)
            .sum()
    }

    pub fn has_back_edge(&self) -> bool {
        self.nodes.iter().any(|node| match node.opcode {
            Opcode::Jump => node.auxiliary0 <= u32::from(node.block_index),
            Opcode::Branch => {
                node.auxiliary0 <= u32::from(node.block_index)
                    || node.auxiliary1 <= u32::from(node.block_index)
            }
            _ => false,
        })
    }

    pub fn loop_budget_literals(&self) -> Vec<u64> {
        self.loop_budgets.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphImport {
    pub name_offset: u32,
    pub name_len: u16,
    pub resource_kind: u16,
    pub rights: u64,
    pub expected_type: PythType,
    pub import_slot: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphBlock {
    pub block_id: u32,
    pub first_node: u32,
    pub node_count: u32,
    pub parameter_count: u16,
    pub terminator_node: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphNode {
    pub opcode: Opcode,
    pub result_type: PythType,
    pub block_index: u16,
    pub inputs: [u32; 4],
    pub auxiliary0: u32,
    pub auxiliary1: u32,
    pub immediate: u64,
}
