use crate::board::{moves::{Move, MoveList, NULL_MOVE}, position::Position, types::Piece};
use crate::eval::evaluate;
use crate::movegen::generate_legal_moves;
use crate::tt::{TT, TT_EXACT, TT_LOWER, TT_UPPER};
use std::time::Instant;

pub const MATE_SCORE: i32 = 30_000;
pub const MATE_THRESHOLD: i32 = 29_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 128;

const MVV: [i32; 6] = [100, 300, 300, 500, 900, 2000];
const LVA: [i32; 6] = [6, 5, 4, 3, 2, 1];

// Reverse-futility margins per depth (static null move pruning)
const RFP_MARGIN: [i32; 9] = [0, 80, 160, 240, 320, 400, 480, 560, 640];
// Futility margins per depth
const FP_MARGIN: [i32; 5] = [0, 100, 200, 300, 400];

pub struct SearchParams {
    pub start: Instant,
    pub soft_limit: Option<u64>,
    pub hard_limit: Option<u64>,
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
        if let Some(lim) = self.params.hard_limit {
            if self.elapsed_ms() >= lim { self.stopped = true; }
        }
        if let Some(lim) = self.params.node_limit {
            if self.nodes >= lim { self.stopped = true; }
        }
    }

    fn score_move(&self, pos: &Position, mv: Move, tt_move: Move, ply: usize) -> i32 {
        if mv == tt_move { return 10_000_000; }
        if mv.is_capture() || mv.is_ep() {
            let victim = if mv.is_ep() { 0 }
                else { pos.piece_at(mv.to()).map(|(_, p)| p as usize).unwrap_or(0) };
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

    fn negamax(
        &mut self,
        pos: &mut Position,
        mut alpha: i32,
        beta: i32,
        depth: i32,
        ply: usize,
        skip_null: bool,
    ) -> i32 {
        self.pv_table[ply].clear();
        self.check_time();
        if self.stopped { return 0; }

        let is_root = ply == 0;
        let is_pv = beta > alpha + 1;

        // Draw detection
        if !is_root {
            if pos.halfmove_clock >= 100 { return 0; }
            if pos.is_repetition() { return 0; }
        }

        // TT probe
        let tt_move = if let Some(e) = self.tt.probe(pos.zobrist) {
            if !is_root && !is_pv && e.depth >= depth as i8 {
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
        let depth = if in_check { depth + 1 } else { depth };

        // Static evaluation for pruning decisions
        let static_eval = if !in_check { evaluate(pos) } else { -INF };

        // Reverse futility pruning (static null move)
        if !is_pv && !in_check && depth <= 8 {
            let margin = RFP_MARGIN[depth as usize];
            if static_eval - margin >= beta {
                return static_eval - margin;
            }
        }

        // Null move pruning
        let has_non_pawns = pos.pieces[pos.side as usize][Piece::Knight as usize]
            | pos.pieces[pos.side as usize][Piece::Bishop as usize]
            | pos.pieces[pos.side as usize][Piece::Rook as usize]
            | pos.pieces[pos.side as usize][Piece::Queen as usize] != 0;

        if !is_pv && !in_check && !skip_null && depth >= 3 && has_non_pawns && static_eval >= beta {
            let r = 2 + depth / 4;
            pos.make_null_move();
            let null_score = -self.negamax(pos, -beta, -beta + 1, depth - 1 - r, ply + 1, true);
            pos.unmake_null_move();
            if self.stopped { return 0; }
            if null_score >= beta {
                // Don't return unverified mate scores
                if null_score >= MATE_THRESHOLD { return beta; }
                return null_score;
            }
        }

        let moves = generate_legal_moves(pos);
        if moves.len == 0 {
            return if in_check { -(MATE_SCORE - ply as i32) } else { 0 };
        }

        let ordered = self.order_moves(pos, &moves, tt_move, ply);
        let orig_alpha = alpha;
        let mut best_score = -INF;
        let mut best_move = NULL_MOVE;
        for (i, mv) in ordered.iter().enumerate() {
            let is_quiet = !mv.is_capture() && !mv.is_ep() && !mv.is_promotion();

            // Futility pruning: skip quiet moves at low depth when we're well below alpha
            if !is_root && !in_check && is_quiet && depth <= 4 && i > 0 {
                let margin = FP_MARGIN[depth as usize];
                if static_eval + margin <= alpha {
                    continue;
                }
            }

            pos.make_move(*mv);
            self.nodes += 1;

            let score = if i == 0 {
                -self.negamax(pos, -beta, -alpha, depth - 1, ply + 1, false)
            } else {
                // Late-move reductions
                let r = if is_quiet && depth >= 3 && i >= 4 {
                    let reduction = 1 + (i as i32).ilog2() as i32;
                    reduction.min(depth - 1)
                } else { 0 };

                // Search with null window first
                let zw = -self.negamax(pos, -alpha - 1, -alpha, depth - 1 - r, ply + 1, false);
                if zw > alpha && (zw < beta || r > 0) {
                    -self.negamax(pos, -beta, -alpha, depth - 1, ply + 1, false)
                } else { zw }
            };

            pos.unmake_move(*mv);
            if self.stopped { return 0; }

            if score > best_score {
                best_score = score;
                best_move = *mv;
                if score > alpha {
                    alpha = score;
                    self.pv_table[ply].clear();
                    self.pv_table[ply].push(*mv);
                    let child = self.pv_table[ply + 1].clone();
                    self.pv_table[ply].extend_from_slice(&child);
                }
            }

            if score >= beta {
                if is_quiet {
                    self.killers[ply][1] = self.killers[ply][0];
                    self.killers[ply][0] = *mv;
                    let h = &mut self.history[mv.from() as usize][mv.to() as usize];
                    *h = (*h + depth * depth).min(10_000);
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

        // Delta pruning: if even capturing a queen can't raise alpha, skip
        const DELTA: i32 = 1025 + 200; // queen value + safety margin
        if stand_pat + DELTA < alpha { return alpha; }

        if stand_pat > alpha { alpha = stand_pat; }

        let moves = generate_legal_moves(pos);
        let ordered = self.order_moves(pos, &moves, NULL_MOVE, ply.min(MAX_PLY - 1));
        for mv in &ordered {
            if !mv.is_capture() && !mv.is_ep() && !mv.is_promotion() { break; } // sorted, quiet at end
            pos.make_move(*mv);
            let score = -self.quiesce(pos, -beta, -alpha, ply + 1);
            pos.unmake_move(*mv);
            if self.stopped { return 0; }
            if score >= beta { return score; }
            if score > alpha { alpha = score; }
        }
        alpha
    }

    /// Age history scores to prevent stale data from dominating.
    fn age_history(&mut self) {
        for row in self.history.iter_mut() {
            for v in row.iter_mut() { *v /= 2; }
        }
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
    let mut prev_score = 0i32;

    for depth in 1..=max_depth {
        searcher.age_history();

        // Aspiration windows (skip at low depths where they're not reliable)
        let score = if depth <= 4 {
            searcher.negamax(pos, -INF, INF, depth, 0, false)
        } else {
            let mut delta = 50i32;
            let mut alpha = (prev_score - delta).max(-INF);
            let mut beta = (prev_score + delta).min(INF);
            loop {
                let s = searcher.negamax(pos, alpha, beta, depth, 0, false);
                if searcher.stopped { break s; }
                if s <= alpha {
                    alpha = (alpha - delta).max(-INF);
                    delta *= 2;
                } else if s >= beta {
                    beta = (beta + delta).min(INF);
                    delta *= 2;
                } else {
                    break s;
                }
            }
        };

        if searcher.stopped && depth > 1 { break; }

        let best = searcher.pv_table[0].first().copied().unwrap_or(NULL_MOVE);
        if !best.is_null() {
            result.best_move = best;
            result.score = score;
            result.depth = depth as u32;
        }
        result.nodes = searcher.nodes;
        prev_score = score;

        let elapsed = searcher.elapsed_ms();
        let nps = if elapsed > 0 { searcher.nodes * 1000 / elapsed } else { searcher.nodes };
        let score_str = if score.abs() > MATE_THRESHOLD {
            let m = (MATE_SCORE - score.abs() + 1) / 2;
            format!("mate {}", if score > 0 { m } else { -m })
        } else {
            format!("cp {}", score)
        };
        let pv: Vec<String> = searcher.pv_table[0].iter().map(|m| m.to_uci()).collect();
        println!("info depth {} score {} nodes {} time {} nps {} pv {}",
            depth, score_str, searcher.nodes, elapsed, nps, pv.join(" "));
        let _ = std::io::Write::flush(&mut std::io::stdout());

        if score.abs() > MATE_THRESHOLD { break; }
        if let Some(soft) = params.soft_limit {
            if searcher.elapsed_ms() >= soft { break; }
        }
    }

    result
}
