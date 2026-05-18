use crate::board::{
    attacks::attackers_to,
    bitboard::pop_lsb,
    moves::{Move, MoveList, NULL_MOVE},
    position::Position,
    types::Piece,
};
use crate::eval::{evaluate, evaluate_quick, evaluate_with_ptable, PawnTable};
use crate::movegen::generate_legal_moves;
use crate::tt::{TT, TT_EXACT, TT_LOWER, TT_UPPER};
use std::time::Instant;

pub const MATE_SCORE: i32 = 30_000;
pub const MATE_THRESHOLD: i32 = 29_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 128;

// SEE piece values (approximate, for exchange evaluation)
const SEE_VAL: [i32; 6] = [100, 300, 300, 500, 900, 20_000];

// MVV-LVA for initial move ordering within winning captures
const MVV: [i32; 6] = [100, 300, 300, 500, 900, 2000];
const LVA: [i32; 6] = [6, 5, 4, 3, 2, 1];

const RFP_MARGIN: [i32; 9] = [0, 80, 160, 240, 320, 400, 480, 560, 640];
const FP_MARGIN: [i32; 5] = [0, 100, 200, 300, 400];
// Late move pruning counts (improving / not-improving)
const LMP_IMPROVING:     [usize; 5] = [0, 10, 16, 24, 32];
const LMP_NOT_IMPROVING: [usize; 5] = [0,  5,  9, 14, 19];
const PROBCUT_MARGIN: i32 = 150;
const LAZY_EVAL_MARGIN: i32 = 300; // skip full eval when quick eval is far outside window
const CORRHIST_SIZE: usize = 16384;
const CORRHIST_GRAIN: i32 = 256;
const CORRHIST_MAX: i32 = 1024 * CORRHIST_GRAIN;

/// Static exchange evaluation. Returns the expected material gain/loss
/// from making `mv` on `pos`, assuming both sides recapture optimally.
fn see(pos: &Position, mv: Move) -> i32 {
    if mv.is_ep() { return 0; }
    let to = mv.to();
    let from = mv.from();

    let captured_val = pos.piece_at(to)
        .map(|(_, p)| SEE_VAL[p as usize])
        .unwrap_or(0);
    let (_, mover_piece) = match pos.piece_at(from) {
        Some(x) => x,
        None => return 0,
    };

    let mut attacker_val = if mv.is_promotion() {
        SEE_VAL[Piece::Queen as usize]
    } else {
        SEE_VAL[mover_piece as usize]
    };

    let mut gain = [0i32; 32];
    gain[0] = captured_val;
    if mv.is_promotion() {
        gain[0] += SEE_VAL[Piece::Queen as usize] - SEE_VAL[Piece::Pawn as usize];
    }

    // Track occupancy and per-color occupancy separately so captured pieces
    // are excluded from subsequent attacker lookups.
    let mut occ = (pos.occupancy[0] | pos.occupancy[1]) ^ (1u64 << from);
    let mut by_color = [pos.occupancy[0], pos.occupancy[1]];
    by_color[pos.side as usize] ^= 1u64 << from;

    let mut stm = pos.side as usize ^ 1; // side making the recapture
    let mut d = 1usize;

    loop {
        // Only consider pieces still on the board (& occ eliminates captured pieces
        // even if pos.pieces still has them, since X-ray sliders use updated occ).
        let all_attackers = attackers_to(to, occ, &pos.pieces) & occ;
        let side_attackers = all_attackers & by_color[stm];
        if side_attackers == 0 { break; }

        // Least valuable attacker
        let (lva_sq, lva_piece) = see_find_lva(side_attackers, &pos.pieces[stm]);
        gain[d] = attacker_val - gain[d - 1];
        attacker_val = SEE_VAL[lva_piece as usize];

        // Remove the attacker; occ update reveals X-ray sliders automatically.
        occ ^= 1u64 << lva_sq;
        by_color[stm] ^= 1u64 << lva_sq;
        stm ^= 1;
        d += 1;
        if d >= 32 { break; }
    }

    // Negamax: neither side is obligated to continue.
    d -= 1;
    while d > 0 {
        gain[d - 1] = (-gain[d]).max(gain[d - 1]);
        d -= 1;
    }
    gain[0]
}

fn see_find_lva(side_attackers: u64, pieces: &[u64; 6]) -> (u32, Piece) {
    for (p, piece_bb) in pieces.iter().enumerate() {
        let overlap = side_attackers & piece_bb;
        if overlap != 0 {
            return (overlap.trailing_zeros(), Piece::from_index(p));
        }
    }
    (0, Piece::King)
}

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
    cont_hist: Box<[[i32; 64]; 64]>,
    // Capture history indexed by [from][to]
    cap_hist: Box<[[i32; 64]; 64]>,
    // Countermove heuristic: indexed by [from][to] of opponent's last move
    countermoves: Box<[[Move; 64]; 64]>,
    // Precomputed LMR table [depth][move_index]
    lmr: [[i32; 64]; 64],
    // Static eval at each ply (for improving flag)
    eval_stack: [i32; MAX_PLY],
    pv_table: Vec<Vec<Move>>,
    prev_move: [Move; MAX_PLY],
    stopped: bool,
    pawn_table: PawnTable,
    corrhist: Box<[[i32; CORRHIST_SIZE]; 2]>,
    cont_hist2: Box<[[i32; 64]; 64]>,
    root_best_nodes: u64,
    root_total_nodes: u64,
}

impl<'a> Search<'a> {
    fn new(params: &'a SearchParams, tt: &'a mut TT) -> Self {
        // Precompute LMR table
        let mut lmr = [[0i32; 64]; 64];
        for d in 1..64usize {
            for i in 1..64usize {
                lmr[d][i] = ((d as f32).ln() * (i as f32).ln() / 2.0) as i32;
            }
        }
        Search {
            params,
            tt,
            nodes: 0,
            killers: [[NULL_MOVE; 2]; MAX_PLY],
            history: Box::new([[0i32; 64]; 64]),
            cont_hist: Box::new([[0i32; 64]; 64]),
            cap_hist: Box::new([[0i32; 64]; 64]),
            countermoves: Box::new([[NULL_MOVE; 64]; 64]),
            lmr,
            eval_stack: [0i32; MAX_PLY],
            pv_table: vec![Vec::new(); MAX_PLY + 1],
            prev_move: [NULL_MOVE; MAX_PLY],
            stopped: false,
            pawn_table: PawnTable::new(),
            corrhist: Box::new([[0i32; CORRHIST_SIZE]; 2]),
            cont_hist2: Box::new([[0i32; 64]; 64]),
            root_best_nodes: 0,
            root_total_nodes: 0,
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

    fn corrected_static_eval(&self, pos: &Position) -> i32 {
        let raw = evaluate(pos);
        let idx = pos.pawn_zobrist as usize % CORRHIST_SIZE;
        let corr = self.corrhist[pos.side as usize][idx] / CORRHIST_GRAIN;
        (raw + corr).clamp(-MATE_THRESHOLD + 1, MATE_THRESHOLD - 1)
    }

    fn update_corrhist(&mut self, pos: &Position, depth: i32, best_score: i32, raw_eval: i32) {
        if best_score.abs() >= MATE_THRESHOLD || raw_eval.abs() >= MATE_THRESHOLD { return; }
        let delta = best_score - raw_eval;
        let weight = (depth + 1).min(8);
        let idx = pos.pawn_zobrist as usize % CORRHIST_SIZE;
        let h = &mut self.corrhist[pos.side as usize][idx];
        let update = delta * CORRHIST_GRAIN * weight;
        *h += (update - *h * update.abs() / CORRHIST_MAX) / 8;
        *h = (*h).clamp(-CORRHIST_MAX, CORRHIST_MAX);
    }

    fn hist_score(&self, mv: Move, ply: usize) -> i32 {
        let h = self.history[mv.from() as usize][mv.to() as usize];
        let pm = self.prev_move[ply.saturating_sub(1)];
        let ch = if !pm.is_null() {
            self.cont_hist[pm.to() as usize][mv.to() as usize]
        } else { 0 };
        let pm2 = if ply >= 2 { self.prev_move[ply - 2] } else { NULL_MOVE };
        let ch2 = if !pm2.is_null() {
            self.cont_hist2[pm2.to() as usize][mv.to() as usize]
        } else { 0 };
        h + ch + ch2
    }

    fn score_move(&self, pos: &Position, mv: Move, tt_move: Move, ply: usize) -> i32 {
        if mv == tt_move { return 10_000_000; }
        if mv.is_capture() || mv.is_ep() {
            let see_score = see(pos, mv);
            let victim = if mv.is_ep() { 0 }
                else { pos.piece_at(mv.to()).map(|(_, p)| p as usize).unwrap_or(0) };
            let attacker = pos.piece_at(mv.from()).map(|(_, p)| p as usize).unwrap_or(0);
            let mvvlva = MVV[victim] - LVA[attacker];
            let ch = self.cap_hist[mv.from() as usize][mv.to() as usize];
            return if see_score >= 0 {
                2_000_000 + mvvlva + ch / 32
            } else {
                -2_000_000 + mvvlva + ch / 32
            };
        }
        if mv.is_promotion() { return 1_900_000; }
        if mv == self.killers[ply][0] { return 800_000; }
        if mv == self.killers[ply][1] { return 700_000; }
        // Countermove heuristic
        let pm = self.prev_move[ply.saturating_sub(1)];
        if !pm.is_null() && mv == self.countermoves[pm.from() as usize][pm.to() as usize] {
            return 600_000;
        }
        self.hist_score(mv, ply)
    }

    fn order_moves(&self, pos: &Position, moves: &MoveList, tt_move: Move, ply: usize) -> Vec<Move> {
        let mut scored: Vec<(i32, Move)> = moves.iter()
            .map(|mv| (self.score_move(pos, mv, tt_move, ply), mv))
            .collect();
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, mv)| mv).collect()
    }

    fn update_history(&mut self, mv: Move, depth: i32, ply: usize, bonus: bool) {
        let delta = if bonus { depth * depth } else { -(depth * depth) };
        let h = &mut self.history[mv.from() as usize][mv.to() as usize];
        *h += delta - *h * delta.abs() / 10_000;

        let pm = self.prev_move[ply.saturating_sub(1)];
        if !pm.is_null() {
            let ch = &mut self.cont_hist[pm.to() as usize][mv.to() as usize];
            *ch += delta - *ch * delta.abs() / 10_000;
        }

        let pm2 = if ply >= 2 { self.prev_move[ply - 2] } else { NULL_MOVE };
        if !pm2.is_null() {
            let ch2 = &mut self.cont_hist2[pm2.to() as usize][mv.to() as usize];
            *ch2 += delta - *ch2 * delta.abs() / 10_000;
        }
    }

    fn update_cap_hist(&mut self, mv: Move, depth: i32, bonus: bool) {
        let delta = if bonus { depth * depth } else { -(depth * depth) };
        let h = &mut self.cap_hist[mv.from() as usize][mv.to() as usize];
        *h += delta - *h * delta.abs() / 10_000;
    }

    fn negamax(
        &mut self,
        pos: &mut Position,
        mut alpha: i32,
        beta: i32,
        mut depth: i32,
        ply: usize,
        skip_null: bool,
        skip_move: Option<Move>, // for singular extensions
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
            if !is_root && skip_move.is_none() && e.depth >= depth as i8 {
                let s = tt_score_from(e.score as i32, ply);
                if !is_pv {
                    match e.flag {
                        TT_EXACT => return s,
                        TT_LOWER if s >= beta => return s,
                        TT_UPPER if s <= alpha => return s,
                        _ => {}
                    }
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
        if in_check { depth += 1; }

        // Internal iterative reduction: no TT move at high depths → search cheaper
        if depth >= 4 && tt_move.is_null() && !in_check {
            depth -= 1;
        }

        // Static evaluation + improving flag
        let raw_eval = if !in_check {
            // Lazy evaluation: use quick eval first; only do full eval if needed
            let quick = evaluate_quick(pos);
            if !is_pv && !skip_null
                && (quick > beta + LAZY_EVAL_MARGIN || quick < alpha - LAZY_EVAL_MARGIN)
            {
                quick
            } else {
                evaluate_with_ptable(pos, &mut self.pawn_table)
            }
        } else { -INF };
        let static_eval = if !in_check {
            let idx = pos.pawn_zobrist as usize % CORRHIST_SIZE;
            let corr = self.corrhist[pos.side as usize][idx] / CORRHIST_GRAIN;
            (raw_eval + corr).clamp(-MATE_THRESHOLD + 1, MATE_THRESHOLD - 1)
        } else { -INF };
        self.eval_stack[ply] = static_eval;
        let improving = !in_check && ply >= 2 && static_eval > self.eval_stack[ply - 2];

        // Reverse futility pruning (be more aggressive when not improving)
        if !is_pv && !in_check && depth <= 8 && skip_move.is_none() {
            let margin = RFP_MARGIN[depth as usize] - if improving { 40 } else { 0 };
            if static_eval - margin >= beta {
                return static_eval - margin;
            }
        }

        // Null move pruning
        let has_non_pawns = pos.pieces[pos.side as usize][Piece::Knight as usize]
            | pos.pieces[pos.side as usize][Piece::Bishop as usize]
            | pos.pieces[pos.side as usize][Piece::Rook as usize]
            | pos.pieces[pos.side as usize][Piece::Queen as usize] != 0;

        if !is_pv && !in_check && !skip_null && depth >= 3 && has_non_pawns
            && static_eval >= beta && skip_move.is_none()
        {
            let r = 3 + depth / 4;
            pos.make_null_move();
            let null_score = -self.negamax(pos, -beta, -beta + 1, depth - 1 - r, ply + 1, true, None);
            pos.unmake_null_move();
            if self.stopped { return 0; }
            if null_score >= beta {
                if null_score >= MATE_THRESHOLD { return beta; }
                return null_score;
            }
        }

        let moves = generate_legal_moves(pos);
        if moves.len == 0 {
            return if in_check { -(MATE_SCORE - ply as i32) } else { 0 };
        }

        // Probcut: at non-PV nodes, try good captures at reduced depth
        if !is_pv && !in_check && depth >= 5 && skip_move.is_none()
            && beta.abs() < MATE_THRESHOLD
        {
            let pc_beta = beta + PROBCUT_MARGIN;
            for i in 0..moves.len {
                let mv = moves.moves[i];
                if !mv.is_capture() && !mv.is_ep() { continue; }
                if see(pos, mv) < pc_beta - static_eval { continue; }
                pos.make_move(mv);
                self.nodes += 1;
                let qs = -self.quiesce(pos, -pc_beta, -pc_beta + 1, ply + 1);
                let score = if qs >= pc_beta && !self.stopped {
                    -self.negamax(pos, -pc_beta, -pc_beta + 1, depth - 4, ply + 1, false, None)
                } else { qs };
                pos.unmake_move(mv);
                if self.stopped { return 0; }
                if score >= pc_beta { return score; }
            }
        }

        // Razoring: at low depths, if static eval is far below alpha, do qsearch
        if !is_pv && !in_check && depth <= 2 && skip_move.is_none() {
            if static_eval + 350 + 150 * depth < alpha {
                let qs = self.quiesce(pos, alpha, beta, ply);
                if qs < alpha { return qs; }
            }
        }

        // Singular extensions: if TT move exists and may be singular, verify
        let (singular_ext, double_ext) = if !is_root && !in_check && skip_move.is_none()
            && depth >= 6 && !tt_move.is_null()
            && {
                let tt_ok = self.tt.probe(pos.zobrist)
                    .map(|e| e.depth >= (depth - 3) as i8 && e.flag != TT_UPPER)
                    .unwrap_or(false);
                tt_ok
            }
        {
            let tt_score = self.tt.probe(pos.zobrist)
                .map(|e| tt_score_from(e.score as i32, ply))
                .unwrap_or(0);
            let s_beta = (tt_score - depth * 2).max(-MATE_SCORE);
            let s_score = self.negamax(pos, s_beta - 1, s_beta, depth / 2, ply, skip_null, Some(tt_move));
            if self.stopped { return 0; }
            let is_singular = s_score < s_beta;
            let is_double = is_singular && s_score < s_beta - 15;
            (is_singular, is_double)
        } else { (false, false) };

        let ordered = self.order_moves(pos, &moves, tt_move, ply);

        // Multi-cut: if multiple moves fail high at reduced depth, prune
        if !is_pv && !in_check && depth >= 7 && skip_move.is_none()
            && beta.abs() < MATE_THRESHOLD && static_eval >= beta
        {
            let mc_r = 3i32;
            let mc_c = 3usize;
            let mut mc_count = 0usize;
            for mv in ordered.iter().take(8) {
                if skip_move == Some(*mv) { continue; }
                pos.make_move(*mv);
                self.nodes += 1;
                let s = -self.negamax(pos, -beta, -beta + 1, depth - 1 - mc_r, ply + 1, false, None);
                pos.unmake_move(*mv);
                if self.stopped { return 0; }
                if s >= beta {
                    mc_count += 1;
                    if mc_count >= mc_c { return beta; }
                }
            }
        }

        let orig_alpha = alpha;
        let mut best_score = -INF;
        let mut best_move = NULL_MOVE;
        let mut quiets_tried = 0usize;

        for (i, mv) in ordered.iter().enumerate() {
            // Skip the excluded move for singular extension search
            if skip_move == Some(*mv) { continue; }

            let is_quiet = !mv.is_capture() && !mv.is_ep() && !mv.is_promotion();
            let is_capture = mv.is_capture() || mv.is_ep();
            let see_score = if is_capture { see(pos, *mv) } else { 0 };

            // --- Pruning (not at root, not in check) ---
            if !is_root && !in_check && best_score > -MATE_THRESHOLD {
                let lmp_limit = if improving { LMP_IMPROVING } else { LMP_NOT_IMPROVING };

                // Futility pruning: quiet moves at low depth when well below alpha
                if is_quiet && depth <= 4 && i > 0 {
                    let margin = FP_MARGIN[depth as usize] + if improving { 30 } else { 0 };
                    if static_eval + margin <= alpha { continue; }
                }

                // Late move pruning
                if is_quiet && depth <= 4 && quiets_tried >= lmp_limit[depth as usize] {
                    continue;
                }

                // History pruning: skip quiet moves with very negative history
                if is_quiet && depth <= 3 {
                    let hs = self.hist_score(*mv, ply);
                    if hs < -2000 * depth { continue; }
                }

                // SEE pruning at low depths
                if depth <= 6 {
                    if is_capture && see_score < -50 * depth { continue; }
                    if is_quiet && see_score < -60 * depth { continue; }
                }
            }

            if is_quiet { quiets_tried += 1; }

            // Extensions
            let pm = self.prev_move[ply.saturating_sub(1)];
            let is_recapture = is_capture && !pm.is_null()
                && (pm.is_capture() || pm.is_ep()) && pm.to() == mv.to()
                && see_score >= 0;
            let ext = if singular_ext && *mv == tt_move {
                if double_ext { 2 } else { 1 }
            } else if is_recapture && depth <= 7 { 1 }
            else { 0 };

            let nodes_before = if is_root { self.nodes } else { 0 };
            pos.make_move(*mv);
            self.nodes += 1;
            self.prev_move[ply] = *mv;

            let score = if i == 0 {
                -self.negamax(pos, -beta, -alpha, depth - 1 + ext, ply + 1, false, None)
            } else {
                // Late-move reductions (use precomputed LMR table)
                let mut r = if (is_quiet || is_capture && see_score < 0) && depth >= 3 && i >= 3 {
                    let base = self.lmr[depth.min(63) as usize][i.min(63)];
                    let hs = self.hist_score(*mv, ply);
                    let hist_adj = (hs / 4000).clamp(-2, 2);
                    let mut reduction = (base - hist_adj).max(0).min(depth - 1);
                    if !improving { reduction += 1; }
                    if is_pv && reduction > 1 { reduction -= 1; }
                    reduction
                } else { 0 };

                // Don't reduce good captures or moves with singular extensions
                if is_capture && see_score >= 0 { r = 0; }
                if ext > 0 { r = 0; }

                let zw = -self.negamax(pos, -alpha - 1, -alpha, depth - 1 - r + ext, ply + 1, false, None);
                if zw > alpha && (zw < beta || r > 0) {
                    -self.negamax(pos, -beta, -alpha, depth - 1 + ext, ply + 1, false, None)
                } else { zw }
            };

            pos.unmake_move(*mv);
            if self.stopped { return 0; }

            if is_root {
                let mv_nodes = self.nodes - nodes_before;
                self.root_total_nodes += mv_nodes;
                if score > best_score {
                    self.root_best_nodes = mv_nodes;
                }
            }

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
                    self.update_history(*mv, depth, ply, true);
                    for &tried in ordered[..i].iter() {
                        if !tried.is_capture() && !tried.is_ep() && !tried.is_promotion() {
                            self.update_history(tried, depth, ply, false);
                        }
                    }
                    // Countermove: record this quiet move as a response to opponent's last move
                    let pm = self.prev_move[ply.saturating_sub(1)];
                    if !pm.is_null() {
                        self.countermoves[pm.from() as usize][pm.to() as usize] = *mv;
                    }
                } else if is_capture {
                    self.update_cap_hist(*mv, depth, true);
                    for &tried in ordered[..i].iter() {
                        if tried.is_capture() || tried.is_ep() {
                            self.update_cap_hist(tried, depth, false);
                        }
                    }
                }
                break;
            }
        }

        let flag = if best_score <= orig_alpha { TT_UPPER }
                   else if best_score >= beta { TT_LOWER }
                   else { TT_EXACT };
        if skip_move.is_none() {
            self.tt.store(pos.zobrist, tt_score_to(best_score, ply) as i16, depth as i8, flag, best_move);
            if !in_check && !self.stopped && depth >= 1 {
                self.update_corrhist(pos, depth, best_score, raw_eval);
            }
        }

        best_score
    }

    fn quiesce(&mut self, pos: &mut Position, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.check_time();
        if self.stopped { return 0; }
        self.nodes += 1;

        let in_check = pos.in_check();

        // TT probe in qsearch
        let tt_move = if let Some(e) = self.tt.probe(pos.zobrist) {
            let s = tt_score_from(e.score as i32, ply);
            if e.depth >= 0 {
                match e.flag {
                    TT_EXACT => return s,
                    TT_LOWER if s >= beta => return s,
                    TT_UPPER if s <= alpha => return s,
                    _ => {}
                }
            }
            e.best_move
        } else { NULL_MOVE };

        let stand_pat = if !in_check {
            let sp = evaluate_with_ptable(pos, &mut self.pawn_table);
            if sp >= beta { return sp; }
            // Delta pruning
            const DELTA: i32 = 1025 + 200;
            if sp + DELTA < alpha { return alpha; }
            if sp > alpha { alpha = sp; }
            sp
        } else { -INF };

        let moves = generate_legal_moves(pos);
        // In check with no moves = checkmate
        if in_check && moves.len == 0 {
            return -(MATE_SCORE - ply as i32);
        }

        let ordered = self.order_moves(pos, &moves, tt_move, ply.min(MAX_PLY - 1));
        let orig_alpha = alpha;
        let mut best_score = if in_check { -INF } else { stand_pat };
        let mut best_move = NULL_MOVE;

        for mv in &ordered {
            // When not in check: only captures, EP, promotions (sorted, quiets break early)
            if !in_check && !mv.is_capture() && !mv.is_ep() && !mv.is_promotion() { break; }
            // Skip bad captures (SEE < 0) when not in check
            if !in_check && see(pos, *mv) < 0 { continue; }
            pos.make_move(*mv);
            let score = -self.quiesce(pos, -beta, -alpha, ply + 1);
            pos.unmake_move(*mv);
            if self.stopped { return 0; }
            if score > best_score {
                best_score = score;
                best_move = *mv;
                if score > alpha {
                    alpha = score;
                    if score >= beta {
                        self.tt.store(pos.zobrist, tt_score_to(score, ply) as i16,
                            0, TT_LOWER, best_move);
                        return score;
                    }
                }
            }
        }

        let flag = if best_score <= orig_alpha { TT_UPPER } else { TT_EXACT };
        self.tt.store(pos.zobrist, tt_score_to(best_score, ply) as i16, 0, flag, best_move);
        best_score
    }

    fn age_history(&mut self) {
        for row in self.history.iter_mut() { for v in row.iter_mut() { *v /= 2; } }
        for row in self.cont_hist.iter_mut() { for v in row.iter_mut() { *v /= 2; } }
        for row in self.cont_hist2.iter_mut() { for v in row.iter_mut() { *v /= 2; } }
        for row in self.cap_hist.iter_mut() { for v in row.iter_mut() { *v /= 2; } }
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
    let mut prev_best = NULL_MOVE;
    let mut stability = 0u32;

    for depth in 1..=max_depth {
        searcher.age_history();

        let score = if depth <= 4 {
            searcher.negamax(pos, -INF, INF, depth, 0, false, None)
        } else {
            let mut delta = 25i32;
            let mut alpha = (prev_score - delta).max(-INF);
            let mut beta = (prev_score + delta).min(INF);
            loop {
                let s = searcher.negamax(pos, alpha, beta, depth, 0, false, None);
                if searcher.stopped { break s; }
                if s <= alpha {
                    alpha = (alpha - delta).max(-INF);
                    beta = (alpha + beta) / 2 + 1; // widen symmetrically
                    delta = (delta * 3 / 2).min(INF);
                } else if s >= beta {
                    beta = (beta + delta).min(INF);
                    delta = (delta * 3 / 2).min(INF);
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

        // Score drop (positive means score got worse this depth)
        let score_drop = prev_score - score;

        // Track best move stability for time scaling
        if best == prev_best && !best.is_null() {
            stability += 1;
        } else {
            stability = 0;
            prev_best = best;
        }
        prev_score = score;

        let elapsed = searcher.elapsed_ms();
        let nps = if elapsed > 0 { searcher.nodes * 1000 / elapsed } else { searcher.nodes };
        let hashfull = searcher.tt.hashfull();
        let score_str = if score.abs() > MATE_THRESHOLD {
            let m = (MATE_SCORE - score.abs() + 1) / 2;
            format!("mate {}", if score > 0 { m } else { -m })
        } else {
            format!("cp {}", score)
        };
        let pv: Vec<String> = searcher.pv_table[0].iter().map(|m| m.to_uci()).collect();
        println!("info depth {} score {} nodes {} time {} nps {} hashfull {} pv {}",
            depth, score_str, searcher.nodes, elapsed, nps, hashfull, pv.join(" "));
        let _ = std::io::Write::flush(&mut std::io::stdout());

        if score.abs() > MATE_THRESHOLD { break; }
        if let Some(soft) = params.soft_limit {
            let stability_scale = match stability {
                0 => 160u64,
                1 => 120,
                2 => 100,
                3 => 85,
                _ => 70,
            };
            let drop_scale = if score_drop > 60 { 160u64 }
                             else if score_drop > 30 { 130u64 }
                             else { 100u64 };
            // Node-based scaling: if best move consumed most nodes, we're confident
            let node_scale = if searcher.root_total_nodes > 0 {
                let frac = searcher.root_best_nodes * 100 / searcher.root_total_nodes;
                if frac > 80 { 60u64 } else if frac > 60 { 80u64 }
                else if frac < 20 { 150u64 } else if frac < 40 { 120u64 }
                else { 100u64 }
            } else { 100u64 };
            // Reset root node counters for next iteration
            searcher.root_best_nodes = 0;
            searcher.root_total_nodes = 0;
            let combined = (stability_scale * drop_scale / 100 * node_scale / 100).clamp(40, 280);
            if searcher.elapsed_ms() * 100 >= soft * combined { break; }
        }
    }

    result
}
