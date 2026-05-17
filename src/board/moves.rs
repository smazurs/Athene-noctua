use super::types::{sq_name, Square};

/// 16-bit move encoding:
/// bits 0-5:   from square
/// bits 6-11:  to square
/// bits 12-15: flags
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Move(pub u16);

pub const FLAG_QUIET: u16 = 0;
pub const FLAG_DOUBLE_PUSH: u16 = 1;
pub const FLAG_CASTLE_KS: u16 = 2;
pub const FLAG_CASTLE_QS: u16 = 3;
pub const FLAG_CAPTURE: u16 = 4;
pub const FLAG_EP: u16 = 5;
// Promotions: 8-15 (bit 3 = promo flag, bit 2 = capture flag, bits 0-1 = promo piece)
pub const FLAG_PROMO_N: u16 = 8;
pub const FLAG_PROMO_B: u16 = 9;
pub const FLAG_PROMO_R: u16 = 10;
pub const FLAG_PROMO_Q: u16 = 11;
pub const FLAG_PROMO_CAPTURE_N: u16 = 12;
pub const FLAG_PROMO_CAPTURE_B: u16 = 13;
pub const FLAG_PROMO_CAPTURE_R: u16 = 14;
pub const FLAG_PROMO_CAPTURE_Q: u16 = 15;

pub const NULL_MOVE: Move = Move(0);

impl Move {
    #[inline(always)]
    pub fn new(from: Square, to: Square, flags: u16) -> Move {
        Move(from as u16 | ((to as u16) << 6) | (flags << 12))
    }

    #[inline(always)]
    pub fn from(self) -> Square {
        (self.0 & 0x3F) as Square
    }

    #[inline(always)]
    pub fn to(self) -> Square {
        ((self.0 >> 6) & 0x3F) as Square
    }

    #[inline(always)]
    pub fn flags(self) -> u16 {
        self.0 >> 12
    }

    #[inline(always)]
    pub fn is_capture(self) -> bool {
        self.flags() & 4 != 0
    }

    #[inline(always)]
    pub fn is_ep(self) -> bool {
        self.flags() == FLAG_EP
    }

    #[inline(always)]
    pub fn is_castle(self) -> bool {
        self.flags() == FLAG_CASTLE_KS || self.flags() == FLAG_CASTLE_QS
    }

    #[inline(always)]
    pub fn is_promotion(self) -> bool {
        self.flags() & 8 != 0
    }

    #[inline(always)]
    pub fn is_double_push(self) -> bool {
        self.flags() == FLAG_DOUBLE_PUSH
    }

    /// Promotion piece type: 0=N, 1=B, 2=R, 3=Q (valid only if is_promotion).
    #[inline(always)]
    pub fn promo_piece(self) -> u8 {
        (self.flags() & 3) as u8
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    pub fn to_uci(self) -> String {
        if self.is_null() {
            return "0000".to_string();
        }
        let mut s = format!("{}{}", sq_name(self.from()), sq_name(self.to()));
        if self.is_promotion() {
            let p = match self.promo_piece() {
                0 => 'n',
                1 => 'b',
                2 => 'r',
                _ => 'q',
            };
            s.push(p);
        }
        s
    }
}

impl std::fmt::Debug for Move {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uci())
    }
}

/// Stack-allocated move list (capacity 256).
pub struct MoveList {
    pub moves: [Move; 256],
    pub len: usize,
}

impl MoveList {
    #[inline(always)]
    pub fn new() -> Self {
        MoveList { moves: [Move(0); 256], len: 0 }
    }

    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        self.moves[self.len] = m;
        self.len += 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = Move> + '_ {
        self.moves[..self.len].iter().copied()
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}
