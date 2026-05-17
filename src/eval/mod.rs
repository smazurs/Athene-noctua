use crate::board::{
    bitboard::{pop_lsb, FILE_A, RANK_1, RANK_8},
    position::Position,
    types::{rank_of, file_of, Color, Piece},
};

// ── Material values ──────────────────────────────────────────────────────────
const MG_MAT: [i32; 6] = [82, 337, 365, 477, 1025, 0];
const EG_MAT: [i32; 6] = [94, 281, 297, 512, 936, 0];
const PHASE_INC: [i32; 6] = [0, 1, 1, 2, 4, 0];
const TOTAL_PHASE: i32 = 24;

// ── Piece-square tables (a1=0, h8=63, white's perspective) ───────────────────
#[rustfmt::skip]
const MG_PAWN: [i32; 64] = [
    0,   0,   0,   0,   0,   0,   0,   0,
    5,  10,  10, -20, -20,  10,  10,   5,
    5,  -5, -10,   0,   0, -10,  -5,   5,
    0,   0,   0,  20,  20,   0,   0,   0,
    5,   5,  10,  25,  25,  10,   5,   5,
   10,  10,  20,  30,  30,  20,  10,  10,
   50,  50,  50,  50,  50,  50,  50,  50,
    0,   0,   0,   0,   0,   0,   0,   0,
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
  -50, -40, -30, -30, -30, -30, -40, -50,
  -40, -20,   0,   5,   5,   0, -20, -40,
  -30,   5,  10,  15,  15,  10,   5, -30,
  -30,   0,  15,  20,  20,  15,   0, -30,
  -30,   5,  15,  20,  20,  15,   5, -30,
  -30,   0,  10,  15,  15,  10,   0, -30,
  -40, -20,   0,   0,   0,   0, -20, -40,
  -50, -40, -30, -30, -30, -30, -40, -50,
];
#[rustfmt::skip]
const MG_BISHOP: [i32; 64] = [
  -20, -10, -10, -10, -10, -10, -10, -20,
  -10,   5,   0,   0,   0,   0,   5, -10,
  -10,  10,  10,  10,  10,  10,  10, -10,
  -10,   0,  10,  10,  10,  10,   0, -10,
  -10,   5,   5,  10,  10,   5,   5, -10,
  -10,   0,   5,  10,  10,   5,   0, -10,
  -10,   0,   0,   0,   0,   0,   0, -10,
  -20, -10, -10, -10, -10, -10, -10, -20,
];
#[rustfmt::skip]
const MG_ROOK: [i32; 64] = [
    0,   0,   0,   5,   5,   0,   0,   0,
   -5,   0,   0,   0,   0,   0,   0,  -5,
   -5,   0,   0,   0,   0,   0,   0,  -5,
   -5,   0,   0,   0,   0,   0,   0,  -5,
   -5,   0,   0,   0,   0,   0,   0,  -5,
   -5,   0,   0,   0,   0,   0,   0,  -5,
    5,  10,  10,  10,  10,  10,  10,   5,
    0,   0,   0,   0,   0,   0,   0,   0,
];
#[rustfmt::skip]
const MG_QUEEN: [i32; 64] = [
  -20, -10, -10,  -5,  -5, -10, -10, -20,
  -10,   0,   5,   0,   0,   0,   0, -10,
  -10,   5,   5,   5,   5,   5,   0, -10,
    0,   0,   5,   5,   5,   5,   0,  -5,
   -5,   0,   5,   5,   5,   5,   0,  -5,
  -10,   0,   5,   5,   5,   5,   0, -10,
  -10,   0,   0,   0,   0,   0,   0, -10,
  -20, -10, -10,  -5,  -5, -10, -10, -20,
];
#[rustfmt::skip]
const MG_KING: [i32; 64] = [
   20,  30,  10,   0,   0,  10,  30,  20,
   20,  20,   0,   0,   0,   0,  20,  20,
  -10, -20, -20, -20, -20, -20, -20, -10,
  -20, -30, -30, -40, -40, -30, -30, -20,
  -30, -40, -40, -50, -50, -40, -40, -30,
  -30, -40, -40, -50, -50, -40, -40, -30,
  -30, -40, -40, -50, -50, -40, -40, -30,
  -30, -40, -40, -50, -50, -40, -40, -30,
];
#[rustfmt::skip]
const EG_KING: [i32; 64] = [
  -50, -30, -30, -30, -30, -30, -30, -50,
  -30, -30,   0,   0,   0,   0, -30, -30,
  -30, -10,  20,  30,  30,  20, -10, -30,
  -30, -10,  30,  40,  40,  30, -10, -30,
  -30, -10,  30,  40,  40,  30, -10, -30,
  -30, -10,  20,  30,  30,  20, -10, -30,
  -30, -20, -10,   0,   0, -10, -20, -30,
  -50, -40, -30, -20, -20, -30, -40, -50,
];

const MG_PST: [&[i32; 64]; 6] = [
    &MG_PAWN, &MG_KNIGHT, &MG_BISHOP, &MG_ROOK, &MG_QUEEN, &MG_KING,
];
const EG_PST: [&[i32; 64]; 6] = [
    &EG_PAWN, &MG_KNIGHT, &MG_BISHOP, &MG_ROOK, &MG_QUEEN, &EG_KING,
];

// ── Passed pawn bonuses by rank (white: rank 0 = rank 1) ─────────────────────
const PASSED_MG: [i32; 8] = [0, 10, 15, 25, 40, 65, 100, 0];
const PASSED_EG: [i32; 8] = [0, 20, 30, 50, 75, 110, 160, 0];

// ── Penalty tables ────────────────────────────────────────────────────────────
const DOUBLED_MG: i32 = 10;
const DOUBLED_EG: i32 = 20;
const ISOLATED_MG: i32 = 15;
const ISOLATED_EG: i32 = 10;
const BISHOP_PAIR_MG: i32 = 30;
const BISHOP_PAIR_EG: i32 = 45;
const ROOK_OPEN_MG: i32 = 25;
const ROOK_SEMIOPEN_MG: i32 = 12;
const ROOK_SEVENTH_MG: i32 = 20;
const ROOK_SEVENTH_EG: i32 = 30;

/// Bitboard of all squares on the same file and adjacent files strictly
/// above `sq` (for white; for black mirror with sq^56).
fn passed_mask_white(sq: u32) -> u64 {
    let file = file_of(sq);
    let mut files = FILE_A << file;
    if file > 0 { files |= FILE_A << (file - 1); }
    if file < 7 { files |= FILE_A << (file + 1); }
    // Mask to ranks strictly above sq
    let rank = rank_of(sq);
    files & (!0u64 << (8 * (rank + 1)))
}

fn eval_pawns(pos: &Position) -> (i32, i32) {
    let wp = pos.pieces[0][Piece::Pawn as usize];
    let bp = pos.pieces[1][Piece::Pawn as usize];
    let mut mg = 0i32;
    let mut eg = 0i32;

    // White pawns
    let mut bb = wp;
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        let rank = rank_of(sq) as usize;
        let file = file_of(sq);
        let file_mask = FILE_A << file;

        // Passed pawn
        if bp & passed_mask_white(sq) == 0 {
            mg += PASSED_MG[rank];
            eg += PASSED_EG[rank];
        }
        // Doubled pawn (another white pawn on same file above)
        if (wp ^ (1u64 << sq)) & file_mask != 0 {
            mg -= DOUBLED_MG;
            eg -= DOUBLED_EG;
        }
        // Isolated pawn
        let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
                | if file < 7 { FILE_A << (file + 1) } else { 0 };
        if wp & adj == 0 {
            mg -= ISOLATED_MG;
            eg -= ISOLATED_EG;
        }
    }

    // Black pawns (mirror: flip ranks for passed mask)
    let mut bb = bp;
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        let rank = rank_of(sq ^ 56) as usize; // mirrored rank
        let file = file_of(sq);
        let file_mask = FILE_A << file;

        if wp & passed_mask_white(sq ^ 56) == 0 {
            mg -= PASSED_MG[rank];
            eg -= PASSED_EG[rank];
        }
        if (bp ^ (1u64 << sq)) & file_mask != 0 {
            mg += DOUBLED_MG;
            eg += DOUBLED_EG;
        }
        let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
                | if file < 7 { FILE_A << (file + 1) } else { 0 };
        if bp & adj == 0 {
            mg += ISOLATED_MG;
            eg += ISOLATED_EG;
        }
    }

    (mg, eg)
}

fn eval_rooks(pos: &Position) -> (i32, i32) {
    let wp = pos.pieces[0][Piece::Pawn as usize];
    let bp = pos.pieces[1][Piece::Pawn as usize];
    let mut mg = 0i32;
    let mut eg = 0i32;

    let mut rooks = pos.pieces[0][Piece::Rook as usize];
    while rooks != 0 {
        let sq = pop_lsb(&mut rooks);
        let file = FILE_A << file_of(sq);
        if wp & file == 0 {
            mg += if bp & file == 0 { ROOK_OPEN_MG } else { ROOK_SEMIOPEN_MG };
        }
        if rank_of(sq) == 6 { mg += ROOK_SEVENTH_MG; eg += ROOK_SEVENTH_EG; }
    }

    let mut rooks = pos.pieces[1][Piece::Rook as usize];
    while rooks != 0 {
        let sq = pop_lsb(&mut rooks);
        let file = FILE_A << file_of(sq);
        if bp & file == 0 {
            mg -= if wp & file == 0 { ROOK_OPEN_MG } else { ROOK_SEMIOPEN_MG };
        }
        if rank_of(sq) == 1 { mg -= ROOK_SEVENTH_MG; eg -= ROOK_SEVENTH_EG; }
    }

    (mg, eg)
}

/// Simple king pawn shield: bonus for pawns in front of the king.
fn eval_king_safety(pos: &Position, phase: i32) -> i32 {
    if phase < 8 { return 0; } // skip in endgame
    let mut mg = 0i32;

    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        let king_sq = pos.king_sq(if color == 0 { Color::White } else { Color::Black });
        let pawns = pos.pieces[color][Piece::Pawn as usize];
        let kfile = file_of(king_sq) as i32;
        let krank = rank_of(king_sq) as i32;
        let forward = if color == 0 { 1i32 } else { -1 };

        // Only apply king safety when king is near the corners (has castled)
        if kfile >= 2 && kfile <= 5 { continue; }

        let mut shield = 0i32;
        for df in -1i32..=1 {
            let f = kfile + df;
            if f < 0 || f > 7 { continue; }
            let r1 = krank + forward;
            let r2 = krank + forward * 2;
            let pawn_on_r1 = r1 >= 0 && r1 < 8 && pawns & (1u64 << (r1 * 8 + f)) != 0;
            let pawn_on_r2 = r2 >= 0 && r2 < 8 && pawns & (1u64 << (r2 * 8 + f)) != 0;
            shield += if pawn_on_r1 { 12 } else if pawn_on_r2 { 5 } else { -20 };
        }
        mg += sign * shield;
    }

    // Scale by phase (full in midgame, zero in endgame)
    mg * phase / TOTAL_PHASE
}

/// Evaluate the position from the side-to-move's perspective.
pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0i32;
    let mut eg = 0i32;
    let mut phase = 0i32;

    // Material + PST
    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        for piece in 0..6usize {
            let mut bb = pos.pieces[color][piece];
            while bb != 0 {
                let sq = pop_lsb(&mut bb) as usize;
                let pst_sq = if color == 0 { sq } else { sq ^ 56 };
                mg += sign * (MG_MAT[piece] + MG_PST[piece][pst_sq]);
                eg += sign * (EG_MAT[piece] + EG_PST[piece][pst_sq]);
                phase += PHASE_INC[piece];
            }
        }
    }

    // Bishop pair
    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        if pos.pieces[color][Piece::Bishop as usize].count_ones() >= 2 {
            mg += sign * BISHOP_PAIR_MG;
            eg += sign * BISHOP_PAIR_EG;
        }
    }

    // Pawn structure
    let (pmg, peg) = eval_pawns(pos);
    mg += pmg;
    eg += peg;

    // Rooks
    let (rmg, reg) = eval_rooks(pos);
    mg += rmg;
    eg += reg;

    phase = phase.min(TOTAL_PHASE);

    // King safety (phase-weighted, only midgame)
    mg += eval_king_safety(pos, phase);

    let score = (mg * phase + eg * (TOTAL_PHASE - phase)) / TOTAL_PHASE;
    if pos.side == Color::White { score } else { -score }
}
