/// Type alias for a 64-bit bitboard.
pub type Bitboard = u64;

pub const EMPTY: Bitboard = 0;
pub const FULL: Bitboard = !0;

pub const FILE_A: Bitboard = 0x0101010101010101;
pub const FILE_B: Bitboard = FILE_A << 1;
pub const FILE_C: Bitboard = FILE_A << 2;
pub const FILE_D: Bitboard = FILE_A << 3;
pub const FILE_E: Bitboard = FILE_A << 4;
pub const FILE_F: Bitboard = FILE_A << 5;
pub const FILE_G: Bitboard = FILE_A << 6;
pub const FILE_H: Bitboard = FILE_A << 7;

pub const NOT_FILE_A: Bitboard = !FILE_A;
pub const NOT_FILE_H: Bitboard = !FILE_H;
pub const NOT_FILE_AB: Bitboard = !FILE_A & !FILE_B;
pub const NOT_FILE_GH: Bitboard = !FILE_G & !FILE_H;

pub const RANK_1: Bitboard = 0x00000000000000FF;
pub const RANK_2: Bitboard = RANK_1 << 8;
pub const RANK_3: Bitboard = RANK_1 << 16;
pub const RANK_4: Bitboard = RANK_1 << 24;
pub const RANK_5: Bitboard = RANK_1 << 32;
pub const RANK_6: Bitboard = RANK_1 << 40;
pub const RANK_7: Bitboard = RANK_1 << 48;
pub const RANK_8: Bitboard = RANK_1 << 56;

#[inline(always)]
pub fn lsb(bb: Bitboard) -> u32 {
    bb.trailing_zeros()
}

#[inline(always)]
pub fn pop_lsb(bb: &mut Bitboard) -> u32 {
    let sq = lsb(*bb);
    *bb &= *bb - 1;
    sq
}

#[inline(always)]
pub fn more_than_one(bb: Bitboard) -> bool {
    bb & (bb - 1) != 0
}

#[inline(always)]
pub fn popcount(bb: Bitboard) -> u32 {
    bb.count_ones()
}

#[inline(always)]
pub fn bit(sq: u32) -> Bitboard {
    1u64 << sq
}

/// Shift north (towards rank 8).
#[inline(always)]
pub fn north(bb: Bitboard) -> Bitboard {
    bb << 8
}

/// Shift south (towards rank 1).
#[inline(always)]
pub fn south(bb: Bitboard) -> Bitboard {
    bb >> 8
}

#[inline(always)]
pub fn east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) << 1
}

#[inline(always)]
pub fn west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) >> 1
}

#[inline(always)]
pub fn north_east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) << 9
}

#[inline(always)]
pub fn north_west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) << 7
}

#[inline(always)]
pub fn south_east(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_H) >> 7
}

#[inline(always)]
pub fn south_west(bb: Bitboard) -> Bitboard {
    (bb & NOT_FILE_A) >> 9
}
