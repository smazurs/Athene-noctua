use crate::board::{
    attacks::{bishop_attacks, king_attacks, knight_attacks, queen_attacks, rook_attacks},
    bitboard::{pop_lsb, FILE_A, FILE_H},
    position::Position,
    types::{file_of, rank_of, Color, Piece},
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

// ── Passed pawn bonuses by rank ───────────────────────────────────────────────
const PASSED_MG: [i32; 8] = [0, 10, 15, 25, 40, 65, 100, 0];
const PASSED_EG: [i32; 8] = [0, 20, 30, 50, 75, 110, 160, 0];

// ── Pawn structure penalties ──────────────────────────────────────────────────
const DOUBLED_MG: i32 = 10;
const DOUBLED_EG: i32 = 20;
const ISOLATED_MG: i32 = 15;
const ISOLATED_EG: i32 = 10;

// ── Piece bonuses ─────────────────────────────────────────────────────────────
const BISHOP_PAIR_MG: i32 = 30;
const BISHOP_PAIR_EG: i32 = 45;
const ROOK_OPEN_MG: i32 = 25;
const ROOK_SEMIOPEN_MG: i32 = 12;
const ROOK_SEVENTH_MG: i32 = 20;
const ROOK_SEVENTH_EG: i32 = 30;

// ── Mobility tables (bonus per number of reachable squares) ──────────────────
const KNIGHT_MOB_MG: [i32; 9]  = [-25, -15, -5,  0,  5, 10, 15, 20, 23];
const KNIGHT_MOB_EG: [i32; 9]  = [-30, -18, -6,  0,  6, 12, 18, 24, 28];
const BISHOP_MOB_MG: [i32; 14] = [-20, -10, -5,  0,  3,  6,  9, 12, 14, 16, 18, 20, 22, 24];
const BISHOP_MOB_EG: [i32; 14] = [-25, -12, -6,  0,  4,  8, 12, 16, 20, 24, 28, 32, 36, 38];
const ROOK_MOB_MG:   [i32; 15] = [-10,  -5,  0,  3,  6,  9, 12, 15, 18, 21, 24, 27, 30, 33, 36];
const ROOK_MOB_EG:   [i32; 15] = [-20, -10,  0,  5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60];
const QUEEN_MOB_MG: [i32; 28]  = [
    -20, -12, -8, -4, 0, 2, 4, 6, 8, 10, 11, 12, 13, 14,
     14,  15, 15, 16, 16, 17, 17, 17, 18, 18, 18, 18, 18, 18,
];
const QUEEN_MOB_EG: [i32; 28]  = [
    -40, -25, -16, -8, 0, 5, 10, 14, 18, 22, 26, 29, 32, 35,
     38,  40,  42, 44, 45, 46, 47, 48, 49, 50, 50, 50, 50, 50,
];

// ── King attack weights (knight, bishop, rook, queen) ────────────────────────
const KING_ATK_WT: [i32; 4] = [2, 2, 3, 5];

// ── Pawn threat bonus (indexed by piece type attacked) ───────────────────────
const PAWN_THREAT: [i32; 6] = [0, 40, 40, 60, 80, 0];

// ── Outpost bonuses [knight, bishop] ─────────────────────────────────────────
const OUTPOST_MG: [i32; 2] = [22, 10];
const OUTPOST_EG: [i32; 2] = [12, 6];

// ── Connected rooks bonus ─────────────────────────────────────────────────────
const CONNECTED_ROOKS_MG: i32 = 10;
const CONNECTED_ROOKS_EG: i32 = 5;

// ── Backward pawn penalties ───────────────────────────────────────────────────
const BACKWARD_MG: i32 = 12;
const BACKWARD_EG: i32 = 15;

// ── Bad bishop (own pawns on same color as bishop) ────────────────────────────
// Light squares: (rank+file) odd; dark squares: (rank+file) even
const LIGHT_SQUARES: u64 = 0xAA55AA55AA55AA55;
const DARK_SQUARES:  u64 = 0x55AA55AA55AA55AA;
const BAD_BISHOP_EG: i32 = 4;  // penalty per own pawn on same color as bishop

// ── Tempo bonus (side to move small advantage) ────────────────────────────────
const TEMPO: i32 = 15;

// ── King proximity bonus per rank/file (endgame only) ────────────────────────
// Closer king to enemy pieces = better in endgame
const KING_PAWN_PROX_EG: i32 = 5;  // per step of Chebyshev distance saved
const KING_KING_PROX_EG: i32 = 2;  // close kings are good for the stronger side

#[inline]
fn chebyshev(a: u32, b: u32) -> i32 {
    let dr = (rank_of(a) as i32 - rank_of(b) as i32).abs();
    let df = (file_of(a) as i32 - file_of(b) as i32).abs();
    dr.max(df)
}

/// Bulk pawn attack bitboard for all pawns of given color.
fn pawn_attack_bb(pawns: u64, color: usize) -> u64 {
    if color == 0 {
        ((pawns & !FILE_A) << 7) | ((pawns & !FILE_H) << 9)
    } else {
        ((pawns & !FILE_H) >> 7) | ((pawns & !FILE_A) >> 9)
    }
}

fn backward_support_mask_white(sq: u32) -> u64 {
    let file = file_of(sq);
    let rank = rank_of(sq);
    let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
            | if file < 7 { FILE_A << (file + 1) } else { 0 };
    let rank_mask = (1u64 << ((rank + 1) * 8)).wrapping_sub(1);
    adj & rank_mask
}

fn backward_support_mask_black(sq: u32) -> u64 {
    let file = file_of(sq);
    let rank = rank_of(sq);
    let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
            | if file < 7 { FILE_A << (file + 1) } else { 0 };
    let rank_mask = !0u64 << (rank * 8);
    adj & rank_mask
}

fn passed_mask_white(sq: u32) -> u64 {
    let file = file_of(sq);
    let mut files = FILE_A << file;
    if file > 0 { files |= FILE_A << (file - 1); }
    if file < 7 { files |= FILE_A << (file + 1); }
    files & (!0u64 << (8 * (rank_of(sq) + 1)))
}

fn eval_pawns(pos: &Position) -> (i32, i32) {
    let wp = pos.pieces[0][Piece::Pawn as usize];
    let bp = pos.pieces[1][Piece::Pawn as usize];
    let mut mg = 0i32; let mut eg = 0i32;

    let wp_attacks = pawn_attack_bb(wp, 0);
    let bp_attacks = pawn_attack_bb(bp, 1);

    let mut bb = wp;
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        let rank = rank_of(sq) as usize;
        let file = file_of(sq);
        let file_mask = FILE_A << file;
        if bp & passed_mask_white(sq) == 0 { mg += PASSED_MG[rank]; eg += PASSED_EG[rank]; }
        if (wp ^ (1u64 << sq)) & file_mask != 0 { mg -= DOUBLED_MG; eg -= DOUBLED_EG; }
        let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
                | if file < 7 { FILE_A << (file + 1) } else { 0 };
        if wp & adj == 0 { mg -= ISOLATED_MG; eg -= ISOLATED_EG; }
        // Backward pawn: square in front attacked by black pawn, no supporting pawn behind
        if sq < 56 {
            let stop = 1u64 << (sq + 8);
            if bp_attacks & stop != 0 && wp & backward_support_mask_white(sq) == 0 {
                mg -= BACKWARD_MG; eg -= BACKWARD_EG;
            }
        }
    }

    let mut bb = bp;
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        let rank = rank_of(sq ^ 56) as usize;
        let file = file_of(sq);
        let file_mask = FILE_A << file;
        if wp & passed_mask_white(sq ^ 56) == 0 { mg -= PASSED_MG[rank]; eg -= PASSED_EG[rank]; }
        if (bp ^ (1u64 << sq)) & file_mask != 0 { mg += DOUBLED_MG; eg += DOUBLED_EG; }
        let adj = if file > 0 { FILE_A << (file - 1) } else { 0 }
                | if file < 7 { FILE_A << (file + 1) } else { 0 };
        if bp & adj == 0 { mg += ISOLATED_MG; eg += ISOLATED_EG; }
        // Backward pawn for black
        if sq >= 8 {
            let stop = 1u64 << (sq - 8);
            if wp_attacks & stop != 0 && bp & backward_support_mask_black(sq) == 0 {
                mg += BACKWARD_MG; eg += BACKWARD_EG;
            }
        }
    }
    (mg, eg)
}

fn eval_rooks(pos: &Position) -> (i32, i32) {
    let wp = pos.pieces[0][Piece::Pawn as usize];
    let bp = pos.pieces[1][Piece::Pawn as usize];
    let mut mg = 0i32; let mut eg = 0i32;

    let mut rooks = pos.pieces[0][Piece::Rook as usize];
    while rooks != 0 {
        let sq = pop_lsb(&mut rooks);
        let file = FILE_A << file_of(sq);
        if wp & file == 0 { mg += if bp & file == 0 { ROOK_OPEN_MG } else { ROOK_SEMIOPEN_MG }; }
        if rank_of(sq) == 6 { mg += ROOK_SEVENTH_MG; eg += ROOK_SEVENTH_EG; }
    }
    let mut rooks = pos.pieces[1][Piece::Rook as usize];
    while rooks != 0 {
        let sq = pop_lsb(&mut rooks);
        let file = FILE_A << file_of(sq);
        if bp & file == 0 { mg -= if wp & file == 0 { ROOK_OPEN_MG } else { ROOK_SEMIOPEN_MG }; }
        if rank_of(sq) == 1 { mg -= ROOK_SEVENTH_MG; eg -= ROOK_SEVENTH_EG; }
    }
    (mg, eg)
}

/// Piece mobility: count reachable squares (excluding own pieces and enemy pawn control for minors).
fn eval_mobility(pos: &Position, occ: u64) -> (i32, i32) {
    let mut mg = 0i32; let mut eg = 0i32;
    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        let own = pos.occupancy[color];
        let opp_ctrl = pawn_attack_bb(pos.pieces[color ^ 1][Piece::Pawn as usize], color ^ 1);

        let mut bb = pos.pieces[color][Piece::Knight as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let mob = (knight_attacks(sq) & !own & !opp_ctrl).count_ones() as usize;
            mg += sign * KNIGHT_MOB_MG[mob.min(8)];
            eg += sign * KNIGHT_MOB_EG[mob.min(8)];
        }
        let mut bb = pos.pieces[color][Piece::Bishop as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let mob = (bishop_attacks(sq, occ) & !own & !opp_ctrl).count_ones() as usize;
            mg += sign * BISHOP_MOB_MG[mob.min(13)];
            eg += sign * BISHOP_MOB_EG[mob.min(13)];
        }
        let mut bb = pos.pieces[color][Piece::Rook as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let mob = (rook_attacks(sq, occ) & !own).count_ones() as usize;
            mg += sign * ROOK_MOB_MG[mob.min(14)];
            eg += sign * ROOK_MOB_EG[mob.min(14)];
        }
        let mut bb = pos.pieces[color][Piece::Queen as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let mob = (queen_attacks(sq, occ) & !own).count_ones() as usize;
            mg += sign * QUEEN_MOB_MG[mob.min(27)];
            eg += sign * QUEEN_MOB_EG[mob.min(27)];
        }
    }
    (mg, eg)
}

/// King safety: pawn shield (when castled) + quadratic penalty for enemy piece attacks on king zone.
fn eval_king_safety(pos: &Position, occ: u64, phase: i32) -> i32 {
    if phase < 4 { return 0; }
    let mut score = 0i32;

    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        let opp = color ^ 1;
        let king_sq = pos.king_sq(if color == 0 { Color::White } else { Color::Black });
        let kfile = file_of(king_sq) as i32;
        let krank = rank_of(king_sq) as i32;
        let forward = if color == 0 { 1i32 } else { -1 };
        let pawns = pos.pieces[color][Piece::Pawn as usize];

        if kfile <= 1 || kfile >= 6 {
            let mut shield = 0i32;
            for df in -1i32..=1 {
                let f = kfile + df;
                if f < 0 || f > 7 { continue; }
                let r1 = krank + forward;
                let r2 = krank + forward * 2;
                let pawn_r1 = r1 >= 0 && r1 < 8 && pawns & (1u64 << (r1 * 8 + f)) != 0;
                let pawn_r2 = r2 >= 0 && r2 < 8 && pawns & (1u64 << (r2 * 8 + f)) != 0;
                shield += if pawn_r1 { 12 } else if pawn_r2 { 5 } else { -20 };
            }
            score += sign * shield;
        }

        let king_zone = king_attacks(king_sq);
        let mut attack_units = 0i32;
        let mut n_attackers = 0i32;

        let mut bb = pos.pieces[opp][Piece::Knight as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            if knight_attacks(sq) & king_zone != 0 { attack_units += KING_ATK_WT[0]; n_attackers += 1; }
        }
        let mut bb = pos.pieces[opp][Piece::Bishop as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            if bishop_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATK_WT[1]; n_attackers += 1; }
        }
        let mut bb = pos.pieces[opp][Piece::Rook as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            if rook_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATK_WT[2]; n_attackers += 1; }
        }
        let mut bb = pos.pieces[opp][Piece::Queen as usize];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            if queen_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATK_WT[3]; n_attackers += 1; }
        }

        if n_attackers >= 2 {
            score -= sign * attack_units * attack_units / 8;
        }
    }
    score * phase / TOTAL_PHASE
}

/// Pawn threats: bonus for pawns attacking enemy pieces.
fn eval_threats(pos: &Position) -> (i32, i32) {
    let mut mg = 0i32; let mut eg = 0i32;

    // White pawns attacking black pieces
    let wp_attacks = pawn_attack_bb(pos.pieces[0][Piece::Pawn as usize], 0);
    for piece in 0..6usize {
        let threatened = (wp_attacks & pos.pieces[1][piece]).count_ones() as i32;
        mg += PAWN_THREAT[piece] * threatened;
        eg += PAWN_THREAT[piece] * threatened;
    }

    // Black pawns attacking white pieces
    let bp_attacks = pawn_attack_bb(pos.pieces[1][Piece::Pawn as usize], 1);
    for piece in 0..6usize {
        let threatened = (bp_attacks & pos.pieces[0][piece]).count_ones() as i32;
        mg -= PAWN_THREAT[piece] * threatened;
        eg -= PAWN_THREAT[piece] * threatened;
    }

    (mg, eg)
}

/// Outpost bonuses for knights and bishops.
fn eval_outposts(pos: &Position) -> (i32, i32) {
    let mut mg = 0i32; let mut eg = 0i32;

    // Masks for outpost ranks: white rank 4-7 (bits 24..63), black rank 1-4 (bits 0..39)
    const OUTPOST_RANKS_WHITE: u64 = 0xFFFF_FFFF_0000_0000u64; // ranks 4-7
    const OUTPOST_RANKS_BLACK: u64 = 0x0000_00FF_FFFF_FFFFu64; // ranks 1-4 (from black's view rank 4-7)

    let wp_attacks = pawn_attack_bb(pos.pieces[0][Piece::Pawn as usize], 0);
    let bp_attacks = pawn_attack_bb(pos.pieces[1][Piece::Pawn as usize], 1);

    // White outpost squares: rank 4-7 and not attacked by black pawns
    let white_outpost_sq = OUTPOST_RANKS_WHITE & !bp_attacks;
    // Black outpost squares: rank 1-4 and not attacked by white pawns
    let black_outpost_sq = OUTPOST_RANKS_BLACK & !wp_attacks;

    // Piece types: knight=1, bishop=2 (indices 0=N, 1=B into OUTPOST arrays)
    for (pi, piece) in [(Piece::Knight as usize, 0usize), (Piece::Bishop as usize, 1usize)] {
        // White pieces
        let mut bb = pos.pieces[0][pi];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let sq_bb = 1u64 << sq;
            if sq_bb & white_outpost_sq != 0 {
                // On outpost
                mg += OUTPOST_MG[piece];
                eg += OUTPOST_EG[piece];
            } else if pi == Piece::Knight as usize {
                // Can reach an outpost in one knight move?
                if knight_attacks(sq) & white_outpost_sq != 0 {
                    mg += OUTPOST_MG[piece] / 2;
                    eg += OUTPOST_EG[piece] / 2;
                }
            }
        }

        // Black pieces
        let mut bb = pos.pieces[1][pi];
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            let sq_bb = 1u64 << sq;
            if sq_bb & black_outpost_sq != 0 {
                mg -= OUTPOST_MG[piece];
                eg -= OUTPOST_EG[piece];
            } else if pi == Piece::Knight as usize {
                if knight_attacks(sq) & black_outpost_sq != 0 {
                    mg -= OUTPOST_MG[piece] / 2;
                    eg -= OUTPOST_EG[piece] / 2;
                }
            }
        }
    }

    (mg, eg)
}

/// Connected rooks bonus: two rooks of same color on same rank/file with no pieces between.
fn eval_connected_rooks(pos: &Position, occ: u64) -> (i32, i32) {
    let mut mg = 0i32; let mut eg = 0i32;

    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        let rooks = pos.pieces[color][Piece::Rook as usize];
        if rooks.count_ones() < 2 { continue; }

        let mut bb = rooks;
        while bb != 0 {
            let sq = pop_lsb(&mut bb);
            // Check if rook attacks land on another friendly rook
            if rook_attacks(sq, occ) & rooks != 0 {
                mg += sign * CONNECTED_ROOKS_MG;
                eg += sign * CONNECTED_ROOKS_EG;
            }
        }
        // Divide by 2 since each pair is counted twice (once from each rook)
        mg = mg; // already counted per-rook, but each pair gives bonus once per rook = 2x; keep as is for now
    }

    // Actually we want to count each connected pair once. Since we counted from both ends,
    // divide by 2. But to keep integer arithmetic simple, we halve below.
    (mg / 2, eg / 2)
}

/// In the endgame, reward the stronger side's king for being close to
/// enemy pawns and close to the enemy king (to restrict it).
fn eval_king_proximity(pos: &Position, phase: i32) -> i32 {
    if phase >= 12 { return 0; } // only meaningful in endgame
    let eg_frac = TOTAL_PHASE - phase; // 0..24
    let wk = pos.king_sq(Color::White);
    let bk = pos.king_sq(Color::Black);
    let mut score = 0i32;

    // White king close to black pawns (attacking them)
    let mut bb = pos.pieces[1][Piece::Pawn as usize];
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        score += (7 - chebyshev(wk, sq)) * KING_PAWN_PROX_EG;
    }
    // Black king close to white pawns
    let mut bb = pos.pieces[0][Piece::Pawn as usize];
    while bb != 0 {
        let sq = pop_lsb(&mut bb);
        score -= (7 - chebyshev(bk, sq)) * KING_PAWN_PROX_EG;
    }
    // Kings closer together → restricts the losing king
    // Bonus for the winning side (approximated by whoever is ahead in eval)
    // We'll apply a symmetric proximity bonus; the caller can decide sign
    let king_dist = chebyshev(wk, bk);
    // Both sides benefit from chasing the enemy king to the edge
    // Use the simple: square of (7-dist) as a proximity bonus to eval
    let _ = king_dist; // used below
    score += (7 - king_dist) * KING_KING_PROX_EG;

    score * eg_frac / TOTAL_PHASE
}

pub fn evaluate(pos: &Position) -> i32 {
    let mut mg = 0i32; let mut eg = 0i32; let mut phase = 0i32;
    let occ = pos.occupancy[0] | pos.occupancy[1];

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

    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        if pos.pieces[color][Piece::Bishop as usize].count_ones() >= 2 {
            mg += sign * BISHOP_PAIR_MG; eg += sign * BISHOP_PAIR_EG;
        }
    }

    // Bad bishop: penalty per own pawn on same color as bishop (endgame term)
    for color in 0..2usize {
        let sign = if color == 0 { 1i32 } else { -1 };
        let bishops = pos.pieces[color][Piece::Bishop as usize];
        let pawns = pos.pieces[color][Piece::Pawn as usize];
        if bishops & LIGHT_SQUARES != 0 {
            let cnt = (pawns & LIGHT_SQUARES).count_ones() as i32;
            eg -= sign * cnt * BAD_BISHOP_EG;
        }
        if bishops & DARK_SQUARES != 0 {
            let cnt = (pawns & DARK_SQUARES).count_ones() as i32;
            eg -= sign * cnt * BAD_BISHOP_EG;
        }
    }

    let (pmg, peg) = eval_pawns(pos);
    mg += pmg; eg += peg;

    let (rmg, reg) = eval_rooks(pos);
    mg += rmg; eg += reg;

    phase = phase.min(TOTAL_PHASE);

    let (mmg, meg) = eval_mobility(pos, occ);
    mg += mmg; eg += meg;

    let (tmg, teg) = eval_threats(pos);
    mg += tmg; eg += teg;

    let (omg, oeg) = eval_outposts(pos);
    mg += omg; eg += oeg;

    let (crmg, creg) = eval_connected_rooks(pos, occ);
    mg += crmg; eg += creg;

    mg += eval_king_safety(pos, occ, phase);

    let score = (mg * phase + eg * (TOTAL_PHASE - phase)) / TOTAL_PHASE;

    // King proximity is purely an endgame term
    let prox = eval_king_proximity(pos, phase);
    let raw = if pos.side == Color::White { score + prox } else { -score - prox };

    // Tempo: small bonus for the side to move
    raw + TEMPO
}
