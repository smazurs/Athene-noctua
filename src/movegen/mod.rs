use crate::board::{
    attacks::{
        bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks, rook_attacks,
    },
    bitboard::*,
    moves::*,
    position::Position,
    types::*,
};

/// Generate all legal moves for the current side.
pub fn generate_legal_moves(pos: &Position) -> MoveList {
    let mut list = MoveList::new();
    generate_moves(pos, &mut list);
    list
}

pub fn generate_moves(pos: &Position, list: &mut MoveList) {
    let us = pos.side;
    let them = us.flip();
    let ui = us as usize;
    let ti = them as usize;

    let our_occ = pos.occupancy[ui];
    let their_occ = pos.occupancy[ti];
    let all = pos.all;
    let empty = !all;

    let king_sq = pos.king_sq(us);

    // Compute checkers
    let checkers = compute_checkers(pos, king_sq, them);
    let num_checkers = checkers.count_ones();

    // In double check, only king moves are legal
    if num_checkers > 1 {
        gen_king_moves(pos, king_sq, us, them, their_occ, all, list);
        return;
    }

    // Compute pinned pieces and check/block mask
    let (pinned, check_mask) = if num_checkers == 1 {
        let checker_sq = lsb(checkers);
        let cm = between(king_sq, checker_sq) | bit(checker_sq);
        (compute_pinned(pos, king_sq, us, them), cm)
    } else {
        (compute_pinned(pos, king_sq, us, them), FULL)
    };

    // Generate moves for each piece type
    gen_pawn_moves(pos, us, them, our_occ, their_occ, all, empty, pinned, check_mask, king_sq, list);
    gen_knight_moves(pos, us, their_occ, pinned, check_mask, list);
    gen_bishop_moves(pos, us, their_occ, all, pinned, check_mask, king_sq, list);
    gen_rook_moves(pos, us, their_occ, all, pinned, check_mask, king_sq, list);
    gen_queen_moves(pos, us, their_occ, all, pinned, check_mask, king_sq, list);
    gen_king_moves(pos, king_sq, us, them, their_occ, all, list);

    if num_checkers == 0 {
        gen_castling(pos, us, all, list);
    }
}

fn compute_checkers(pos: &Position, king_sq: Square, them: Color) -> Bitboard {
    let ti = them as usize;
    let all = pos.all;
    let mut checkers = 0u64;
    checkers |= pawn_attacks(king_sq, them.flip() as usize) & pos.pieces[ti][Piece::Pawn as usize];
    checkers |= knight_attacks(king_sq) & pos.pieces[ti][Piece::Knight as usize];
    checkers |= bishop_attacks(king_sq, all)
        & (pos.pieces[ti][Piece::Bishop as usize] | pos.pieces[ti][Piece::Queen as usize]);
    checkers |= rook_attacks(king_sq, all)
        & (pos.pieces[ti][Piece::Rook as usize] | pos.pieces[ti][Piece::Queen as usize]);
    checkers
}

fn compute_pinned(pos: &Position, king_sq: Square, us: Color, them: Color) -> Bitboard {
    let ui = us as usize;
    let ti = them as usize;
    let our_occ = pos.occupancy[ui];
    let mut pinned = 0u64;

    // X-ray through our pieces to find diagonal pinners
    let diagonal_attackers = pos.pieces[ti][Piece::Bishop as usize]
        | pos.pieces[ti][Piece::Queen as usize];
    let mut pc = bishop_attacks(king_sq, pos.all ^ our_occ) & diagonal_attackers;
    while pc != 0 {
        let pinner = pop_lsb(&mut pc);
        let ray = between(king_sq, pinner) & our_occ;
        if ray != 0 && !more_than_one(ray) {
            pinned |= ray;
        }
    }

    // X-ray through our pieces to find orthogonal pinners
    let orthogonal_attackers = pos.pieces[ti][Piece::Rook as usize]
        | pos.pieces[ti][Piece::Queen as usize];
    let mut pc2 = rook_attacks(king_sq, pos.all ^ our_occ) & orthogonal_attackers;
    while pc2 != 0 {
        let pinner = pop_lsb(&mut pc2);
        let ray = between(king_sq, pinner) & our_occ;
        if ray != 0 && !more_than_one(ray) {
            pinned |= ray;
        }
    }

    pinned
}

/// Squares between two squares along a rank/file/diagonal (exclusive of both ends).
fn between(a: Square, b: Square) -> Bitboard {
    unsafe { BETWEEN_TABLE[a as usize][b as usize] }
}

/// Squares on the ray from a through b (inclusive of b, exclusive of a).
fn ray_through(a: Square, b: Square) -> Bitboard {
    unsafe { RAY_TABLE[a as usize][b as usize] }
}

fn gen_pawn_moves(
    pos: &Position,
    us: Color,
    them: Color,
    _our_occ: Bitboard,
    their_occ: Bitboard,
    all: Bitboard,
    empty: Bitboard,
    pinned: Bitboard,
    check_mask: Bitboard,
    king_sq: Square,
    list: &mut MoveList,
) {
    let ui = us as usize;
    let mut pawns = pos.pieces[ui][Piece::Pawn as usize];

    // For en passant, we'll handle separately
    let ep = pos.ep_square;

    while pawns != 0 {
        let sq = pop_lsb(&mut pawns);
        let bb = bit(sq);
        let is_pinned = bb & pinned != 0;

        if us == Color::White {
            // Push
            let push1 = north(bb) & empty;
            if push1 != 0 {
                let to = lsb(push1);
                let to_bb = bit(to);
                if to_bb & check_mask != 0 {
                    if !is_pinned || is_on_pin_ray(sq, to, king_sq) {
                        if rank_of(to) == 7 {
                            push_promotions(sq, to, false, list);
                        } else {
                            list.push(Move::new(sq, to, FLAG_QUIET));
                        }
                    }
                }
                // Double push
                if rank_of(sq) == 1 {
                    let push2 = north(push1) & empty;
                    if push2 != 0 {
                        let to2 = lsb(push2);
                        if bit(to2) & check_mask != 0 {
                            if !is_pinned || is_on_pin_ray(sq, to2, king_sq) {
                                list.push(Move::new(sq, to2, FLAG_DOUBLE_PUSH));
                            }
                        }
                    }
                }
            }
            // Captures
            let atks = pawn_attacks(sq, us as usize) & their_occ;
            let mut a = atks & check_mask;
            while a != 0 {
                let to = pop_lsb(&mut a);
                if !is_pinned || is_on_pin_ray(sq, to, king_sq) {
                    if rank_of(to) == 7 {
                        push_promotions(sq, to, true, list);
                    } else {
                        list.push(Move::new(sq, to, FLAG_CAPTURE));
                    }
                }
            }
        } else {
            // Black
            let push1 = south(bb) & empty;
            if push1 != 0 {
                let to = lsb(push1);
                let to_bb = bit(to);
                if to_bb & check_mask != 0 {
                    if !is_pinned || is_on_pin_ray(sq, to, king_sq) {
                        if rank_of(to) == 0 {
                            push_promotions(sq, to, false, list);
                        } else {
                            list.push(Move::new(sq, to, FLAG_QUIET));
                        }
                    }
                }
                if rank_of(sq) == 6 {
                    let push2 = south(push1) & empty;
                    if push2 != 0 {
                        let to2 = lsb(push2);
                        if bit(to2) & check_mask != 0 {
                            if !is_pinned || is_on_pin_ray(sq, to2, king_sq) {
                                list.push(Move::new(sq, to2, FLAG_DOUBLE_PUSH));
                            }
                        }
                    }
                }
            }
            // Captures
            let atks = pawn_attacks(sq, us as usize) & their_occ;
            let mut a = atks & check_mask;
            while a != 0 {
                let to = pop_lsb(&mut a);
                if !is_pinned || is_on_pin_ray(sq, to, king_sq) {
                    if rank_of(to) == 0 {
                        push_promotions(sq, to, true, list);
                    } else {
                        list.push(Move::new(sq, to, FLAG_CAPTURE));
                    }
                }
            }
        }

        // En passant
        if ep != squares::NONE {
            let ep_bb = bit(ep);
            if pawn_attacks(sq, us as usize) & ep_bb != 0 {
                // The captured pawn square
                let cap_sq = if us == Color::White { ep - 8 } else { ep + 8 };
                // Verify pawn actually exists at cap_sq
                debug_assert!(
                    pos.pieces[them as usize][0] & bit(cap_sq) != 0,
                    "EP cap_sq {:?} has no enemy pawn! ep={:?} pos={}",
                    cap_sq, ep, pos.to_fen()
                );
                // Check if ep resolves check (captured pawn was the checker)
                if bit(cap_sq) & check_mask != 0 || bit(ep) & check_mask != 0 {
                    // Check that ep doesn't expose king (horizontal pin)
                    if ep_is_legal(pos, sq, ep, cap_sq, king_sq, us) {
                        list.push(Move::new(sq, ep, FLAG_EP));
                    }
                }
            }
        }
    }
}

fn ep_is_legal(pos: &Position, from: Square, ep_to: Square, cap_sq: Square, king_sq: Square, us: Color) -> bool {
    // After en passant, the from and cap_sq pawns are gone, ep_to has our pawn.
    let them = us.flip();
    let ti = them as usize;
    let occ = (pos.all ^ bit(from) ^ bit(cap_sq)) | bit(ep_to);
    // Check if king is now attacked by a slider on the same rank
    let rank = rank_of(king_sq);
    if rank != rank_of(from) {
        // No horizontal exposure possible (different ranks) — still check diagonals via pin
        // For simplicity, simulate the attack
    }
    let rq = pos.pieces[ti][Piece::Rook as usize] | pos.pieces[ti][Piece::Queen as usize];
    let bq = pos.pieces[ti][Piece::Bishop as usize] | pos.pieces[ti][Piece::Queen as usize];
    rook_attacks(king_sq, occ) & rq == 0 && bishop_attacks(king_sq, occ) & bq == 0
}

fn is_on_pin_ray(from: Square, to: Square, king_sq: Square) -> bool {
    // A pinned piece can only move along the pin ray (towards or away from the pinner).
    // A move is on the pin ray if king, from, to are collinear.
    ray_through(king_sq, from) & bit(to) != 0 || ray_through(from, king_sq) & bit(to) != 0
        || from == king_sq // shouldn't happen
}

fn push_promotions(from: Square, to: Square, is_capture: bool, list: &mut MoveList) {
    let base = if is_capture { FLAG_PROMO_CAPTURE_N } else { FLAG_PROMO_N };
    list.push(Move::new(from, to, base));
    list.push(Move::new(from, to, base + 1));
    list.push(Move::new(from, to, base + 2));
    list.push(Move::new(from, to, base + 3));
}

fn gen_knight_moves(
    pos: &Position,
    us: Color,
    their_occ: Bitboard,
    pinned: Bitboard,
    check_mask: Bitboard,
    list: &mut MoveList,
) {
    let ui = us as usize;
    let mut knights = pos.pieces[ui][Piece::Knight as usize] & !pinned;
    while knights != 0 {
        let sq = pop_lsb(&mut knights);
        let mut atks = knight_attacks(sq) & !pos.occupancy[ui] & check_mask;
        while atks != 0 {
            let to = pop_lsb(&mut atks);
            let flag = if bit(to) & their_occ != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
            list.push(Move::new(sq, to, flag));
        }
    }
}

fn gen_bishop_moves(
    pos: &Position,
    us: Color,
    their_occ: Bitboard,
    all: Bitboard,
    pinned: Bitboard,
    check_mask: Bitboard,
    king_sq: Square,
    list: &mut MoveList,
) {
    let ui = us as usize;
    let mut bishops = pos.pieces[ui][Piece::Bishop as usize];
    while bishops != 0 {
        let sq = pop_lsb(&mut bishops);
        let is_pinned = bit(sq) & pinned != 0;
        let mut atks = bishop_attacks(sq, all) & !pos.occupancy[ui] & check_mask;
        if is_pinned {
            atks &= ray_through(king_sq, sq) | ray_through(sq, king_sq);
        }
        while atks != 0 {
            let to = pop_lsb(&mut atks);
            let flag = if bit(to) & their_occ != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
            list.push(Move::new(sq, to, flag));
        }
    }
}

fn gen_rook_moves(
    pos: &Position,
    us: Color,
    their_occ: Bitboard,
    all: Bitboard,
    pinned: Bitboard,
    check_mask: Bitboard,
    king_sq: Square,
    list: &mut MoveList,
) {
    let ui = us as usize;
    let mut rooks = pos.pieces[ui][Piece::Rook as usize];
    while rooks != 0 {
        let sq = pop_lsb(&mut rooks);
        let is_pinned = bit(sq) & pinned != 0;
        let mut atks = rook_attacks(sq, all) & !pos.occupancy[ui] & check_mask;
        if is_pinned {
            atks &= ray_through(king_sq, sq) | ray_through(sq, king_sq);
        }
        while atks != 0 {
            let to = pop_lsb(&mut atks);
            let flag = if bit(to) & their_occ != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
            list.push(Move::new(sq, to, flag));
        }
    }
}

fn gen_queen_moves(
    pos: &Position,
    us: Color,
    their_occ: Bitboard,
    all: Bitboard,
    pinned: Bitboard,
    check_mask: Bitboard,
    king_sq: Square,
    list: &mut MoveList,
) {
    let ui = us as usize;
    let mut queens = pos.pieces[ui][Piece::Queen as usize];
    while queens != 0 {
        let sq = pop_lsb(&mut queens);
        let is_pinned = bit(sq) & pinned != 0;
        let mut atks = queen_attacks(sq, all) & !pos.occupancy[ui] & check_mask;
        if is_pinned {
            atks &= ray_through(king_sq, sq) | ray_through(sq, king_sq);
        }
        while atks != 0 {
            let to = pop_lsb(&mut atks);
            let flag = if bit(to) & their_occ != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
            list.push(Move::new(sq, to, flag));
        }
    }
}

fn gen_king_moves(
    pos: &Position,
    king_sq: Square,
    us: Color,
    them: Color,
    their_occ: Bitboard,
    _all: Bitboard,
    list: &mut MoveList,
) {
    let ui = us as usize;
    // Remove king from occupancy so it doesn't block its own ray attacks
    let occ_no_king = pos.all ^ bit(king_sq);
    let mut atks = king_attacks(king_sq) & !pos.occupancy[ui];
    while atks != 0 {
        let to = pop_lsb(&mut atks);
        if !is_attacked_with_occ(to, them, pos, occ_no_king) {
            let flag = if bit(to) & their_occ != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
            list.push(Move::new(king_sq, to, flag));
        }
    }
}

/// Check if `sq` is attacked by `attacker` given a custom occupancy.
fn is_attacked_with_occ(sq: Square, attacker: Color, pos: &Position, occ: Bitboard) -> bool {
    let ai = attacker as usize;
    if pawn_attacks(sq, attacker.flip() as usize) & pos.pieces[ai][Piece::Pawn as usize] != 0 {
        return true;
    }
    if knight_attacks(sq) & pos.pieces[ai][Piece::Knight as usize] != 0 {
        return true;
    }
    if king_attacks(sq) & pos.pieces[ai][Piece::King as usize] != 0 {
        return true;
    }
    if bishop_attacks(sq, occ)
        & (pos.pieces[ai][Piece::Bishop as usize] | pos.pieces[ai][Piece::Queen as usize])
        != 0
    {
        return true;
    }
    if rook_attacks(sq, occ)
        & (pos.pieces[ai][Piece::Rook as usize] | pos.pieces[ai][Piece::Queen as usize])
        != 0
    {
        return true;
    }
    false
}

fn gen_castling(pos: &Position, us: Color, all: Bitboard, list: &mut MoveList) {
    let them = us.flip();
    // Remove king from occ so it doesn't block slider attacks through castling squares
    let king_sq = pos.king_sq(us);
    let occ_no_king = all ^ bit(king_sq);

    if us == Color::White {
        if pos.castling & CASTLE_WK != 0 {
            if all & 0x60 == 0
                && !is_attacked_with_occ(squares::E1, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::F1, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::G1, them, pos, occ_no_king)
            {
                list.push(Move::new(squares::E1, squares::G1, FLAG_CASTLE_KS));
            }
        }
        if pos.castling & CASTLE_WQ != 0 {
            if all & 0xE == 0
                && !is_attacked_with_occ(squares::E1, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::D1, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::C1, them, pos, occ_no_king)
            {
                list.push(Move::new(squares::E1, squares::C1, FLAG_CASTLE_QS));
            }
        }
    } else {
        if pos.castling & CASTLE_BK != 0 {
            if all & 0x6000000000000000 == 0
                && !is_attacked_with_occ(squares::E8, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::F8, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::G8, them, pos, occ_no_king)
            {
                list.push(Move::new(squares::E8, squares::G8, FLAG_CASTLE_KS));
            }
        }
        if pos.castling & CASTLE_BQ != 0 {
            if all & 0x0E00000000000000 == 0
                && !is_attacked_with_occ(squares::E8, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::D8, them, pos, occ_no_king)
                && !is_attacked_with_occ(squares::C8, them, pos, occ_no_king)
            {
                list.push(Move::new(squares::E8, squares::C8, FLAG_CASTLE_QS));
            }
        }
    }
}

// --------------------------------------------------------------------------
// Between / Ray tables
// --------------------------------------------------------------------------

use std::sync::OnceLock;
static BETWEEN_TABLE_CELL: OnceLock<[[Bitboard; 64]; 64]> = OnceLock::new();
static RAY_TABLE_CELL: OnceLock<[[Bitboard; 64]; 64]> = OnceLock::new();

static mut BETWEEN_TABLE: [[Bitboard; 64]; 64] = [[0; 64]; 64];
static mut RAY_TABLE: [[Bitboard; 64]; 64] = [[0; 64]; 64];

pub fn init_tables() {
    unsafe {
        for a in 0u32..64 {
            for b in 0u32..64 {
                BETWEEN_TABLE[a as usize][b as usize] = compute_between(a, b);
                RAY_TABLE[a as usize][b as usize] = compute_ray(a, b);
            }
        }
    }
}

fn compute_between(a: u32, b: u32) -> Bitboard {
    if a == b {
        return 0;
    }
    let af = (a % 8) as i32;
    let ar = (a / 8) as i32;
    let bf = (b % 8) as i32;
    let br = (b / 8) as i32;
    let df = (bf - af).signum();
    let dr = (br - ar).signum();
    if df == 0 && dr == 0 {
        return 0;
    }
    // Must be on same rank, file, or diagonal
    if df != 0 && dr != 0 && (bf - af).abs() != (br - ar).abs() {
        return 0;
    }
    if df == 0 && af != bf {
        return 0;
    }
    if dr == 0 && ar != br {
        return 0;
    }
    let mut result = 0u64;
    let (mut f, mut r) = (af + df, ar + dr);
    while (f, r) != (bf, br) {
        result |= bit((r * 8 + f) as u32);
        f += df;
        r += dr;
    }
    result
}

fn compute_ray(a: u32, b: u32) -> Bitboard {
    if a == b {
        return 0;
    }
    let af = (a % 8) as i32;
    let ar = (a / 8) as i32;
    let bf = (b % 8) as i32;
    let br = (b / 8) as i32;
    let df = (bf - af).signum();
    let dr = (br - ar).signum();
    if df == 0 && dr == 0 {
        return 0;
    }
    if df != 0 && dr != 0 && (bf - af).abs() != (br - ar).abs() {
        return 0;
    }
    if df == 0 && af != bf {
        return 0;
    }
    if dr == 0 && ar != br {
        return 0;
    }
    let mut result = 0u64;
    let (mut f, mut r) = (af + df, ar + dr);
    while f >= 0 && f < 8 && r >= 0 && r < 8 {
        result |= bit((r * 8 + f) as u32);
        f += df;
        r += dr;
    }
    result
}
