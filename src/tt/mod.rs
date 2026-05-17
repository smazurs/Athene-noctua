/// Transposition table.
use crate::board::moves::Move;

pub const TT_NONE: u8 = 0;
pub const TT_EXACT: u8 = 1;
pub const TT_LOWER: u8 = 2; // failed high (score >= beta)
pub const TT_UPPER: u8 = 3; // failed low  (score <= alpha)

#[derive(Clone, Copy, Default)]
pub struct TTEntry {
    pub key: u64,
    pub score: i16,
    pub depth: i8,
    pub flag: u8,
    pub best_move: Move,
}

pub struct TT {
    table: Vec<TTEntry>,
    mask: usize,
}

impl TT {
    /// Create a TT of approximately `mb` megabytes.
    pub fn new(mb: usize) -> Self {
        let bytes = mb * 1024 * 1024;
        let entry_size = std::mem::size_of::<TTEntry>();
        let mut size = bytes / entry_size;
        // Round down to power of 2
        size = size.next_power_of_two() >> 1;
        if size == 0 { size = 1; }
        TT { table: vec![TTEntry::default(); size], mask: size - 1 }
    }

    pub fn probe(&self, key: u64) -> Option<&TTEntry> {
        let entry = &self.table[key as usize & self.mask];
        if entry.key == key && entry.flag != TT_NONE {
            Some(entry)
        } else {
            None
        }
    }

    pub fn store(&mut self, key: u64, score: i16, depth: i8, flag: u8, best_move: Move) {
        let idx = key as usize & self.mask;
        let entry = &mut self.table[idx];
        // Replace-always with depth-preferred for same key
        if entry.key != key || depth >= entry.depth || flag == TT_EXACT {
            *entry = TTEntry { key, score, depth, flag, best_move };
        }
    }

    pub fn clear(&mut self) {
        self.table.iter_mut().for_each(|e| *e = TTEntry::default());
    }
}
