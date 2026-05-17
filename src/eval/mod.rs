use crate::board::{
    bitboard::popcount,
    position::Position,
    types::{Color, Piece},
};

// Material values [piece] — middlegame and endgame
const MG_MAT: [i32; 6] = [82, 337, 365, 477, 1025, 0];
const EG_MAT: [i32; 6] = [94, 281, 297, 512, 936, 0];

// Phase contribution per piece (knights, bishops cost 1; rooks 2; queens 4)
const PHASE_INC: [i32; 6] = [0, 1, 1, 2, 4, 0];
const TOTAL_PHASE: i32 = 24; // 4N + 4B + 4R + 2Q = 4+4+8+8

// ── Piece-square tables ──────────────────────────────────────────────────────
// Indexed [sq] where sq = rank*8 + file, rank 0 = rank 1 (white's back rank).
// White's perspective; for black, mirror the rank: use index sq^56.
// Based on Tomasz Michniewski's Simplified Evaluation Function.

#[rustfmt::skip]
const MG_PAWN: [i32; 64] = [
    0,   0,   0,   0,   0,   0,   0,   0,  // rank 1
    5,  10,  10, -20, -20,  10,  10,   5,  // rank 2
    5,  -5, -10,   0,   0, -10,  -5,   5,  // rank 3
    0,   0,   0,  20,  20,   0,   0,   0,  // rank 4
    5,   5,  10,  25,  25,  10,   5,   5,  // rank 5
   10,  10,  20,  30,  30,  20,  10,  10,  // rank 6
   50,  50,  50,  50,  50,  50,  50,  50,  // rank 7
    0,   0,   0,   0,   0,   0,   0,   0,  // rank 8
];

#[rustfmt::skip]
const EG_PAWN: [i32; 64] = [
    0,   0,   0,   0,   0,   0,   0,   0,
   -3,  -3,  -3,  -3,  -3,  -3,  -3,  -3,
   -3,  -3,  -3,  -3,  -3,  -3,  -3,  -3,
    5,   5,   5,   5,   5,   5,   5,   5,
   10,  10,  10,  10,  10,  10,  10,  10,
   20,  20,  20,  20,  20,  20,  20,  20,
   50,  50,  50,  50,  50,  50,  50,  50,
    0,   0,   0,   0,   0,   0,   0,   0,
];

#[rustfmt::skip]
const MG_KNIGHT: [i32; 64] = [
  -50, -40, -30, -30, -30, -30, -40, -50,  // rank 1
  -40, -20,   0,   5,   5,   0, -20, -40,  // rank 2
  -30,   5,  10,  15,  15,  10,   5, -30,  // rank 3
  -30,   0,  15,  20,  20,  15,   0, -30,  // rank 4
  -30,   5,  15,  20,  20,  15,   5, -30,  // rank 5
  -30,   0,  10,  15,  15,  10,   0, -30,  // rank 6
  -40, -20,   0,   0,   0,   0, -20, -40,  // rank 7
  -50, -40, -30, -30, -30, -30, -40, -50,  // rank 8
];

#[rustfmt::skip]
const MG_BISHOP: [i32; 64] = [
  -20, -10, -10, -10, -10, -10, -10, -20,  // rank 1
  -10,   5,   0,   0,   0,   0,   5, -10,  // rank 2
  -10,  10,  10,  10,  10,  10,  10, -10,  // rank 3
  -10,   0,  10,  10,  10,  10,   0, -10,  // rank 4
  -10,   5,   5,  10,  10,   5,   5, -10,  // rank 5
  -10,   0,   5,  10,  10,   5,   0, -10,  // rank 6
  -10,   0,   0,   0,   0,   0,   0, -10,  // rank 7
  -20, -10, -10, -10, -10, -10, -10, -20,  // rank 8
];

#[rustfmt::skip]
const MG_ROOK: [i32; 64] = [
    0,   0,   0,   5,   5,   0,   0,   0,  // rank 1
   -5,   0,   0,   0,   0,   0,   0,  -5,  // rank 2
   -5,   0,   0,   0,   0,   0,   0,  -5,  // rank 3
   -5,   0,   0,   0,   0,   0,   0,  -5,  // rank 4
   -5,   0,   0,   0,   0,   0,   0,  -5,  // rank 5
   -5,   0,   0,   0,   0,   0,   0,  -5,  // rank 6
    5,  10,  10,  10,  10,  10,  10,   5,  // rank 7
    0,   0,   0,   0,   0,   0,   0,   0,  // rank 8
];

#[rustfmt::skip]
const MG_QUEEN: [i32; 64] = [
  -20, -10, -10,  -5,  -5, -10, -10, -20,  // rank 1
  -10,   0,   5,   0,   0,   0,   0, -10,  // rank 2
  -10,   5,   5,   5,   5,   5,   0, -10,  // rank 3
    0,   0,   5,   5,   5,   5,   0,  -5,  // rank 4
   -5,   0,   5,   5,   5,   5,   0,  -5,  // rank 5
  -10,   0,   5,   5,   5,   5,   0, -10,  // rank 6
  -10,   0,   0,   0,   0,   0,   0, -10,  // rank 7
  -20, -10, -10,  -5,  -5, -10, -10, -20,  // rank 8
];

#[rustfmt::skip]
const MG_KING: [i32; 64] = [
   20,  30,  10,   0,   0,  10,  30,  20,  // rank 1  (castle safety)
   20,  20,   0,   0,   0,   0,  20,  20,  // rank 2
  -10, -20, -20, -20, -20, -20, -20, -10,  // rank 3
  -20, -30, -30, -40, -40, -30, -30, -20,  // rank 4
  -30, -40, -40, -50, -50, -40, -40, -30,  // rank 5
  -30, -40, -40, -50, -50, -40, -40, -30,  // rank 6
  -30, -40, -40, -50, -50, -40, -40, -30,  // rank 7
  -30, -40, -40, -50, -50, -40, -40, -30,  // rank 8
];

#[rustfmt::skip]
const EG_KING: [i32; 64] = [
  -50, -30, -30, -30, -30, -30, -30, -50,  // rank 1
  -30, -30,   0,   0,   0,   0, -30, -30,  // rank 2
  -30, -10,  20,  30,  30,  20, -10, -30,  // rank 3
  -30, -10,  30,  40,  40,  30, -10, -30,  // rank 4
  -30, -10,  30,  40,  40,  30, -10, -30,  // rank 5
  -30, -10,  20,  30,  30,  20, -10, -30,  // rank 6
  -30, -20, -10,   0,   0, -10, -20, -30,  // rank 7
  -50, -40, -30, -20, -20, -30, -40, -50,  // rank 8
];

const MG_PST: [&[i32; 64]; 6] = [
    &MG_PAWN, &MG_KNIGHT, &MG_BISHOP, &MG_ROOK, &MG_QUEEN, &MG_KING,
];
const EG_PST: [&[i32; 64]; 6] = [
    &EG_PAWN, &MG_KNIGHT, &MG_BISHOP, &MG_ROOK, &MG_QUEEN, &EG_KING,
];

/// Evaluate the position from the side-to-move's perspective (positive = good for stm).
pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0i32;
    let mut eg = 0i32;
    let mut phase = 0i32;

    for color in 0..2 {
        let sign = if color == pos.side as usize { 1 } else { -1 };
        for piece in 0..6 {
            let mut bb = pos.pieces[color][piece];
            while bb != 0 {
                let sq = crate::board::bitboard::pop_lsb(&mut bb) as usize;
                // Mirror sq for black (flip rank)
                let pst_sq = if color == 0 { sq } else { sq ^ 56 };
                mg += sign * (MG_MAT[piece] + MG_PST[piece][pst_sq]);
                eg += sign * (EG_MAT[piece] + EG_PST[piece][pst_sq]);
                phase += PHASE_INC[piece];
            }
        }
    }

    phase = phase.min(TOTAL_PHASE);
    // Taper: blend mg and eg
    (mg * phase + eg * (TOTAL_PHASE - phase)) / TOTAL_PHASE
}
