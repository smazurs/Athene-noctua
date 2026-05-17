// UCI stub — Phase 1 will implement full UCI loop.
use crate::board::position::Position;
use crate::board::moves::Move;
use crate::board::types::{parse_sq, squares, rank_of, Piece, Color};
use crate::board::attacks::{self, bishop_attacks, rook_attacks, queen_attacks, knight_attacks, king_attacks, pawn_attacks};
use crate::board::bitboard::bit;
use crate::board::moves::{
    FLAG_QUIET, FLAG_DOUBLE_PUSH, FLAG_CASTLE_KS, FLAG_CASTLE_QS,
    FLAG_CAPTURE, FLAG_EP,
    FLAG_PROMO_N, FLAG_PROMO_B, FLAG_PROMO_R, FLAG_PROMO_Q,
    FLAG_PROMO_CAPTURE_N, FLAG_PROMO_CAPTURE_B, FLAG_PROMO_CAPTURE_R, FLAG_PROMO_CAPTURE_Q,
};
use crate::board::types::{CASTLE_WK, CASTLE_WQ, CASTLE_BK, CASTLE_BQ};
use crate::movegen::{generate_legal_moves, init_tables};

pub fn run_uci() {
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    let mut pos = Position::startpos();

    for line in stdin.lock().lines() {
        let line = line.expect("stdin error");
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.splitn(2, ' ');
        let cmd = tokens.next().unwrap_or("");
        let rest = tokens.next().unwrap_or("");

        match cmd {
            "uci" => {
                println!("id name Athene-noctua");
                println!("id author Athene-noctua Authors");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => {
                pos = Position::startpos();
            }
            "position" => {
                pos = parse_position(rest);
            }
            "go" => {
                // Phase 0: no search yet, just report that we're not ready
                println!("bestmove 0000");
            }
            "stop" => {}
            "quit" => return,
            "perft" => {
                let depth: u32 = rest.trim().parse().unwrap_or(1);
                let nodes = perft(&mut pos, depth);
                println!("Nodes: {}", nodes);
            }
            _ => {}
        }
    }
}

fn parse_position(rest: &str) -> Position {
    let mut tokens = rest.split_whitespace();
    let mut pos = match tokens.next() {
        Some("startpos") => Position::startpos(),
        Some("fen") => {
            let fen: String = tokens.by_ref().take(6).collect::<Vec<_>>().join(" ");
            Position::from_fen(&fen).unwrap_or_else(|_| Position::startpos())
        }
        _ => Position::startpos(),
    };
    if tokens.next() == Some("moves") {
        for mv_str in tokens {
            let mv = parse_uci_move(&pos, mv_str);
            if let Some(m) = mv {
                pos.make_move(m);
            }
        }
    }
    pos
}

pub fn parse_uci_move(pos: &Position, s: &str) -> Option<Move> {
    if s.len() < 4 {
        return None;
    }
    let from = parse_sq(&s[0..2])?;
    let to = parse_sq(&s[2..4])?;
    let promo = s.chars().nth(4);
    let moves = generate_legal_moves(pos);
    for m in moves.iter() {
        if m.from() == from && m.to() == to {
            if let Some(p) = promo {
                if m.is_promotion() {
                    let pp = match p {
                        'n' => 0u8,
                        'b' => 1,
                        'r' => 2,
                        'q' => 3,
                        _ => 3,
                    };
                    if m.promo_piece() == pp {
                        return Some(m);
                    }
                    continue;
                }
            } else if !m.is_promotion() {
                return Some(m);
            }
        }
    }
    None
}

pub fn perft(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = generate_legal_moves(pos);
    if depth == 1 {
        return moves.len as u64;
    }
    let mut nodes = 0u64;
    for m in moves.iter() {
        pos.make_move(m);
        nodes += perft(pos, depth - 1);
        pos.unmake_move(m);
    }
    nodes
}

/// Count moves by type at all leaf positions to find the extra moves.
pub fn count_move_types(pos: &mut Position, depth: u32, counts: &mut [u64; 16]) {
    if depth == 0 {
        return;
    }
    let moves = generate_legal_moves(pos);
    if depth == 1 {
        for m in moves.iter() {
            counts[m.flags() as usize] += 1;
        }
        return;
    }
    for m in moves.iter() {
        pos.make_move(m);
        count_move_types(pos, depth - 1, counts);
        pos.unmake_move(m);
    }
}

/// Find illegal moves by making each move and checking if the king is in check.
pub fn find_illegal_moves(pos: &mut Position, depth: u32) -> u64 {
    use crate::board::moves::{FLAG_CAPTURE, FLAG_EP, FLAG_PROMO_CAPTURE_N, FLAG_PROMO_CAPTURE_Q};
    if depth == 0 {
        return 0;
    }
    let side_before = pos.side;
    let them_i = side_before.flip() as usize;
    let moves = generate_legal_moves(pos);

    // Check for duplicate moves
    let mut seen = std::collections::HashSet::new();
    for m in moves.iter() {
        let key = (m.from(), m.to(), m.flags());
        if !seen.insert(key) {
            eprintln!("DUPLICATE MOVE: {} in {}", m.to_uci(), pos.to_fen());
        }
    }

    let mut count = 0u64;
    for m in moves.iter() {
        // Validate captures: target square must have an enemy piece
        let flags = m.flags();
        if flags == FLAG_CAPTURE || (flags >= FLAG_PROMO_CAPTURE_N && flags <= FLAG_PROMO_CAPTURE_Q) {
            if pos.piece_at(m.to()).map(|(c, _)| c == side_before).unwrap_or(true) {
                eprintln!("PHANTOM CAPTURE: {} in {}", m.to_uci(), pos.to_fen());
            }
        }
        // Check the from square actually has our piece
        if pos.piece_at(m.from()).map(|(c, _)| c != side_before).unwrap_or(true) {
            eprintln!("NO PIECE AT FROM: {} in {}", m.to_uci(), pos.to_fen());
        }

        pos.make_move(m);
        let king_sq = pos.king_sq(side_before);
        if pos.is_attacked(king_sq, pos.side) {
            eprintln!("ILLEGAL MOVE: {} from {}", m.to_uci(), pos.to_fen());
            count += 1;
        } else if depth > 1 {
            count += find_illegal_moves(pos, depth - 1);
        }
        pos.unmake_move(m);
    }
    let _ = them_i;
    count
}

/// Perft that panics on board inconsistency to locate make/unmake bugs.
pub fn perft_validating(pos: &mut Position, depth: u32) -> u64 {
    if let Some(err) = pos.validate() {
        panic!("BOARD INVALID at depth {}: {} | fen={}", depth, err, pos.to_fen());
    }
    if depth == 0 {
        return 1;
    }
    let moves = generate_legal_moves(pos);
    if depth == 1 {
        return moves.len as u64;
    }
    let mut nodes = 0u64;
    for m in moves.iter() {
        pos.make_move(m);
        nodes += perft_validating(pos, depth - 1);
        pos.unmake_move(m);
        if let Some(err) = pos.validate() {
            panic!("BOARD INVALID after unmake {} at depth {}: {} | fen={}",
                m.to_uci(), depth, err, pos.to_fen());
        }
    }
    nodes
}

/// Brute-force: generate ALL pseudo-legal moves (ignoring pins/checks), filter by king safety.
/// Returns the count of legal moves. Panics if different from generate_legal_moves.
fn brute_force_legal_count(pos: &mut Position) -> usize {
    let us = pos.side;
    let ui = us as usize;
    let ti = us.flip() as usize;
    let all = pos.all;
    let our_occ = pos.occupancy[ui];
    let their_occ = pos.occupancy[ti];
    let ep = pos.ep_square;

    let mut pseudo: Vec<Move> = Vec::with_capacity(256);

    let mut bb = our_occ;
    while bb != 0 {
        let sq = crate::board::bitboard::pop_lsb(&mut bb);
        let (_, piece) = pos.piece_at(sq).unwrap();
        match piece {
            Piece::Pawn => {
                let attacks = pawn_attacks(sq, ui);
                if us == Color::White {
                    let to = sq + 8;
                    if to < 64 && all & bit(to) == 0 {
                        if rank_of(to) == 7 {
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_N));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_B));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_R));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_Q));
                        } else {
                            pseudo.push(Move::new(sq, to, FLAG_QUIET));
                        }
                        if rank_of(sq) == 1 {
                            let to2 = sq + 16;
                            if to2 < 64 && all & bit(to2) == 0 {
                                pseudo.push(Move::new(sq, to2, FLAG_DOUBLE_PUSH));
                            }
                        }
                    }
                    let mut cap_bb = attacks & their_occ;
                    while cap_bb != 0 {
                        let to = crate::board::bitboard::pop_lsb(&mut cap_bb);
                        if rank_of(to) == 7 {
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_N));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_B));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_R));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_Q));
                        } else {
                            pseudo.push(Move::new(sq, to, FLAG_CAPTURE));
                        }
                    }
                    if ep != squares::NONE && attacks & bit(ep) != 0 {
                        pseudo.push(Move::new(sq, ep, FLAG_EP));
                    }
                } else {
                    if sq >= 8 {
                        let to = sq - 8;
                        if all & bit(to) == 0 {
                            if rank_of(to) == 0 {
                                pseudo.push(Move::new(sq, to, FLAG_PROMO_N));
                                pseudo.push(Move::new(sq, to, FLAG_PROMO_B));
                                pseudo.push(Move::new(sq, to, FLAG_PROMO_R));
                                pseudo.push(Move::new(sq, to, FLAG_PROMO_Q));
                            } else {
                                pseudo.push(Move::new(sq, to, FLAG_QUIET));
                            }
                            if rank_of(sq) == 6 && sq >= 16 {
                                let to2 = sq - 16;
                                if all & bit(to2) == 0 {
                                    pseudo.push(Move::new(sq, to2, FLAG_DOUBLE_PUSH));
                                }
                            }
                        }
                    }
                    let mut cap_bb = attacks & their_occ;
                    while cap_bb != 0 {
                        let to = crate::board::bitboard::pop_lsb(&mut cap_bb);
                        if rank_of(to) == 0 {
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_N));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_B));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_R));
                            pseudo.push(Move::new(sq, to, FLAG_PROMO_CAPTURE_Q));
                        } else {
                            pseudo.push(Move::new(sq, to, FLAG_CAPTURE));
                        }
                    }
                    if ep != squares::NONE && attacks & bit(ep) != 0 {
                        pseudo.push(Move::new(sq, ep, FLAG_EP));
                    }
                }
            }
            Piece::Knight => {
                let mut atks = knight_attacks(sq) & !our_occ;
                while atks != 0 {
                    let to = crate::board::bitboard::pop_lsb(&mut atks);
                    let f = if their_occ & bit(to) != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
                    pseudo.push(Move::new(sq, to, f));
                }
            }
            Piece::Bishop => {
                let mut atks = bishop_attacks(sq, all) & !our_occ;
                while atks != 0 {
                    let to = crate::board::bitboard::pop_lsb(&mut atks);
                    let f = if their_occ & bit(to) != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
                    pseudo.push(Move::new(sq, to, f));
                }
            }
            Piece::Rook => {
                let mut atks = rook_attacks(sq, all) & !our_occ;
                while atks != 0 {
                    let to = crate::board::bitboard::pop_lsb(&mut atks);
                    let f = if their_occ & bit(to) != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
                    pseudo.push(Move::new(sq, to, f));
                }
            }
            Piece::Queen => {
                let mut atks = queen_attacks(sq, all) & !our_occ;
                while atks != 0 {
                    let to = crate::board::bitboard::pop_lsb(&mut atks);
                    let f = if their_occ & bit(to) != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
                    pseudo.push(Move::new(sq, to, f));
                }
            }
            Piece::King => {
                let mut atks = king_attacks(sq) & !our_occ;
                while atks != 0 {
                    let to = crate::board::bitboard::pop_lsb(&mut atks);
                    let f = if their_occ & bit(to) != 0 { FLAG_CAPTURE } else { FLAG_QUIET };
                    pseudo.push(Move::new(sq, to, f));
                }
                // Castling: only if king not in check, path clear, transit squares not attacked
                let them = us.flip();
                let king_in_check = pos.is_attacked(sq, them);
                if !king_in_check {
                    if us == Color::White {
                        if pos.castling & CASTLE_WK != 0 && all & 0x60 == 0
                            && !pos.is_attacked(squares::F1, them)
                        {
                            pseudo.push(Move::new(squares::E1, squares::G1, FLAG_CASTLE_KS));
                        }
                        if pos.castling & CASTLE_WQ != 0 && all & 0xE == 0
                            && !pos.is_attacked(squares::D1, them)
                        {
                            pseudo.push(Move::new(squares::E1, squares::C1, FLAG_CASTLE_QS));
                        }
                    } else {
                        if pos.castling & CASTLE_BK != 0 && all & 0x6000000000000000 == 0
                            && !pos.is_attacked(squares::F8, them)
                        {
                            pseudo.push(Move::new(squares::E8, squares::G8, FLAG_CASTLE_KS));
                        }
                        if pos.castling & CASTLE_BQ != 0 && all & 0x0E00000000000000 == 0
                            && !pos.is_attacked(squares::D8, them)
                        {
                            pseudo.push(Move::new(squares::E8, squares::C8, FLAG_CASTLE_QS));
                        }
                    }
                }
            }
        }
    }

    // Filter by king safety
    let mut legal = 0usize;
    for m in &pseudo {
        pos.make_move(*m);
        let ksq = pos.king_sq(us);
        if !pos.is_attacked(ksq, us.flip()) {
            legal += 1;
        }
        pos.unmake_move(*m);
    }
    legal
}

/// Perft that cross-checks the fast generator against brute-force at every node.
pub fn perft_crosscheck(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let fast = generate_legal_moves(pos);
    let slow = brute_force_legal_count(pos);
    if slow != fast.len {
        eprintln!("MISMATCH depth {}: fast={} slow={} fen={}", depth, fast.len, slow, pos.to_fen());
    }
    if depth == 1 {
        return fast.len as u64;
    }
    let mut nodes = 0u64;
    for m in fast.iter() {
        pos.make_move(m);
        nodes += perft_crosscheck(pos, depth - 1);
        pos.unmake_move(m);
    }
    nodes
}

pub fn perft_divide(pos: &mut Position, depth: u32) -> u64 {
    let moves = generate_legal_moves(pos);
    let mut total = 0u64;
    for m in moves.iter() {
        pos.make_move(m);
        let n = perft(pos, depth - 1);
        pos.unmake_move(m);
        println!("{}: {}", m.to_uci(), n);
        total += n;
    }
    println!("Total: {}", total);
    total
}
