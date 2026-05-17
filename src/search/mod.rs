use crate::board::{moves::{Move, MoveList, NULL_MOVE}, position::Position, types::Piece};
use crate::eval::evaluate;
use crate::movegen::generate_legal_moves;
use crate::tt::{TT, TT_EXACT, TT_LOWER, TT_UPPER};
use std::time::Instant;

pub const MATE_SCORE: i32 = 30_000;
pub const MATE_THRESHOLD: i32 = 29_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 128;

// MVV-LVA victim value (index = piece type 0..5)
const MVV: [i32; 6] = [100, 300, 300, 500, 900, 2000];
// LVA attacker penalty
const LVA: [i32; 6] = [6, 5, 4, 3, 2, 1];

pub struct SearchParams {
    pub start: Instant,
    pub soft_limit: Option<u64>,  // target ms (stop after this if possible)
    pub hard_limit: Option<u64>,  // abort ms (stop immediately)
    pub depth_limit: Option<u32>,
    pub node_limit: Option<u64>,
}

pub struct SearchResult {
    pub best_move: Move,
    pub score: i32,
    pub depth: u32,
    pub nodes: u64,
}

struct Search<'a> {
    params: &'a SearchParams,
    tt: &'a mut TT,
    nodes: u64,
    killers: [[Move; 2]; MAX_PLY],
    history: Box<[[i32; 64]; 64]>,
    pv_table: Vec<Vec<Move>>,
    stopped: bool,
}

impl<'a> Search<'a> {
    fn new(params: &'a SearchParams, tt: &'a mut TT) -> Self {
        Search {
            params,
            tt,
            nodes: 0,
            killers: [[NULL_MOVE; 2]; MAX_PLY],
            history: Box::new([[0i32; 64]; 64]),
            pv_table: vec![Vec::new(); MAX_PLY + 1],
            stopped: false,
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.params.start.elapsed().as_millis() as u64
    }

    fn check_time(&mut self) {
        if self.nodes & 4095 != 0 { return; }
        if let Some(limit) = self.params.hard_limit {
            if self.elapsed_ms() >= limit { self.stopped = true; }
        }
        if let Some(limit) = self.params.node_limit {
            if self.nodes >= limit { self.stopped = true; }
        }
    }

    fn score_move(&self, pos: &Position, mv: Move, tt_move: Move, ply: usize) -> i32 {
        if mv == tt_move { return 10_000_000; }
        if mv.is_capture() || mv.is_ep() {
            let victim = if mv.is_ep() {
                Piece::Pawn as usize
            } else {
                pos.piece_at(mv.to()).map(|(_, p)| p as usize).unwrap_or(0)
            };
            let attacker = pos.piece_at(mv.from()).map(|(_, p)| p as usize).unwrap_or(0);
            return 1_000_000 + MVV[victim] - LVA[attacker];
        }
        if mv.is_promotion() { return 900_000; }
        if mv == self.killers[ply][0] { return 800_000; }
        if mv == self.killers[ply][1] { return 700_000; }
        self.history[mv.from() as usize][mv.to() as usize]
    }

    fn order_moves(&self, pos: &Position, moves: &MoveList, tt_move: Move, ply: usize) -> Vec<Move> {
        let mut scored: Vec<(i32, Move)> = moves.iter()
            .map(|mv| (self.score_move(pos, mv, tt_move, ply), mv))
            .collect();
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, mv)| mv).collect()
    }

    fn negamax(&mut self, pos: &mut Position, mut alpha: i32, beta: i32, depth: i32, ply: usize) -> i32 {
        self.pv_table[ply].clear();
        self.check_time();
        if self.stopped { return 0; }

        let is_root = ply == 0;
        let is_pv = beta > alpha + 1;

        // Draw detection (fifty-move / repetition skipped for now)
        if !is_root && pos.halfmove_clock >= 100 { return 0; }

        // TT probe
        let tt_move = if let Some(e) = self.tt.probe(pos.zobrist) {
            if !is_root && e.depth >= depth as i8 {
                let s = tt_score_from(e.score as i32, ply);
                match e.flag {
                    TT_EXACT => return s,
                    TT_LOWER if s >= beta => return s,
                    TT_UPPER if s <= alpha => return s,
                    _ => {}
                }
            }
            e.best_move
        } else {
            NULL_MOVE
        };

        if depth <= 0 {
            return self.quiesce(pos, alpha, beta, ply);
        }

        let in_check = pos.in_check();
        // Check extension
        let depth = if in_check { depth + 1 } else { depth };

        let moves = generate_legal_moves(pos);
        if moves.len == 0 {
            return if in_check { -(MATE_SCORE - ply as i32) } else { 0 };
        }

        let ordered = self.order_moves(pos, &moves, tt_move, ply);
        let orig_alpha = alpha;
        let mut best_score = -INF;
        let mut best_move = NULL_MOVE;

        for (i, mv) in ordered.iter().enumerate() {
            pos.make_move(*mv);
            self.nodes += 1;

            // Late-move reductions (simple: reduce quiet moves after first few)
            let score = if i >= 4 && depth >= 3 && !mv.is_capture() && !mv.is_promotion() && !in_check {
                let r = 1 + (i as i32 - 4) / 6;
                let s = -self.negamax(pos, -alpha - 1, -alpha, depth - 1 - r, ply + 1);
                if s > alpha {
                    -self.negamax(pos, -beta, -alpha, depth - 1, ply + 1)
                } else { s }
            } else if i > 0 && is_pv {
                // PVS: search with null window first
                let s = -self.negamax(pos, -alpha - 1, -alpha, depth - 1, ply + 1);
                if s > alpha && s < beta {
                    -self.negamax(pos, -beta, -alpha, depth - 1, ply + 1)
                } else { s }
            } else {
                -self.negamax(pos, -beta, -alpha, depth - 1, ply + 1)
            };

            pos.unmake_move(*mv);
            if self.stopped { return 0; }

            if score > best_score {
                best_score = score;
                best_move = *mv;
                if score > alpha {
                    alpha = score;
                    // Update PV
                    self.pv_table[ply].clear();
                    self.pv_table[ply].push(*mv);
                    let child = self.pv_table[ply + 1].clone();
                    self.pv_table[ply].extend_from_slice(&child);
                }
            }

            if score >= beta {
                if !mv.is_capture() && !mv.is_ep() {
                    self.killers[ply][1] = self.killers[ply][0];
                    self.killers[ply][0] = *mv;
                    let h = &mut self.history[mv.from() as usize][mv.to() as usize];
                    *h += depth * depth;
                    // Cap to avoid overflow
                    if *h > 10_000 { *h = 10_000; }
                }
                break;
            }
        }

        let flag = if best_score <= orig_alpha { TT_UPPER }
                   else if best_score >= beta { TT_LOWER }
                   else { TT_EXACT };
        self.tt.store(pos.zobrist, tt_score_to(best_score, ply) as i16, depth as i8, flag, best_move);

        best_score
    }

    fn quiesce(&mut self, pos: &mut Position, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.check_time();
        if self.stopped { return 0; }
        self.nodes += 1;

        let stand_pat = evaluate(pos);
        if stand_pat >= beta { return stand_pat; }
        if stand_pat > alpha { alpha = stand_pat; }

        let moves = generate_legal_moves(pos);
        let ordered = self.order_moves(pos, &moves, NULL_MOVE, ply.min(MAX_PLY - 1));
        for mv in &ordered {
            if !mv.is_capture() && !mv.is_ep() && !mv.is_promotion() { continue; }
            pos.make_move(*mv);
            let score = -self.quiesce(pos, -beta, -alpha, ply + 1);
            pos.unmake_move(*mv);
            if self.stopped { return 0; }
            if score >= beta { return score; }
            if score > alpha { alpha = score; }
        }
        alpha
    }
}

fn tt_score_to(score: i32, ply: usize) -> i32 {
    if score > MATE_THRESHOLD { score + ply as i32 }
    else if score < -MATE_THRESHOLD { score - ply as i32 }
    else { score }
}

fn tt_score_from(score: i32, ply: usize) -> i32 {
    if score > MATE_THRESHOLD { score - ply as i32 }
    else if score < -MATE_THRESHOLD { score + ply as i32 }
    else { score }
}

pub fn search(pos: &mut Position, params: &SearchParams, tt: &mut TT) -> SearchResult {
    let mut searcher = Search::new(params, tt);
    let max_depth = params.depth_limit.unwrap_or(64) as i32;
    let mut result = SearchResult { best_move: NULL_MOVE, score: 0, depth: 0, nodes: 0 };

    for depth in 1..=max_depth {
        let score = searcher.negamax(pos, -INF, INF, depth, 0);
        if searcher.stopped && depth > 1 { break; }

        let best = searcher.pv_table[0].first().copied().unwrap_or(NULL_MOVE);
        if !best.is_null() {
            result.best_move = best;
            result.score = score;
            result.depth = depth as u32;
        }
        result.nodes = searcher.nodes;

        // Print info line
        let elapsed = searcher.elapsed_ms();
        let nps = if elapsed > 0 { searcher.nodes * 1000 / elapsed } else { searcher.nodes };
        let score_str = if score.abs() > MATE_THRESHOLD {
            let moves_to_mate = (MATE_SCORE - score.abs() + 1) / 2;
            format!("mate {}", if score > 0 { moves_to_mate } else { -moves_to_mate })
        } else {
            format!("cp {}", score)
        };
        let pv: Vec<String> = searcher.pv_table[0].iter().map(|m| m.to_uci()).collect();
        println!("info depth {} score {} nodes {} time {} nps {} pv {}",
            depth, score_str, searcher.nodes, elapsed, nps, pv.join(" "));
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // Stop if we found a forced mate or hit soft time limit
        if score.abs() > MATE_THRESHOLD { break; }
        if let Some(soft) = params.soft_limit {
            if searcher.elapsed_ms() >= soft { break; }
        }
    }

    result
}
