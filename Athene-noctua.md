# Project: Build a World-Class Chess Engine

## Context for Claude Code

You are helping me build a chess engine targeting **3300+ CCRL Elo**, with the architecture and engineering practices used by today's top engines (Stockfish 18, Torch, Obsidian, Berserk, Caissa). The realistic goal is to compete in the global top 30-50 within 6-12 months of disciplined development. Beating Stockfish outright is the stretch goal and requires a genuine algorithmic insight beyond known techniques, so the build order is structured to get to a strong baseline as fast as possible, *then* leave room for experimentation.

**Read this entire document before writing any code.** Then propose a phased plan, confirm the tech stack with me, and only start coding after I approve Phase 1.

## Reference benchmarks (CCRL 40/15, 4 CPU, as of 2026)

```
1.  Stockfish 17/18    ~3640-3650
2.  Torch v3           ~3636
3.  Dragon (Komodo)    ~3627
4.  Obsidian 14        ~3618
5.  Berserk 13         ~3616
6.  PlentyChess 2.1    ~3611
7.  Caissa 1.20        ~3610
... (top 30 all sit between 3550 and 3650)
```

The gap between #1 and #30 is roughly 100 Elo. The gap from a clean-room "Phase 1 working engine" to top 30 is roughly 1300 Elo.

---

## Tech stack

**Language: Rust** (preferred) or **C++17/20**.

Reasoning: Rust gets you memory safety, zero-cost abstractions, excellent SIMD (`std::simd` / `wide` / `packed_simd`), and a healthy modern engine ecosystem (Viridithas, Carp, Akimbo, Stormphrax-ish ports). C++ is the historical standard and what Stockfish uses, with marginally better compiler maturity for chess workloads. Python is a non-starter for the engine itself, though we will use PyTorch for NNUE training.

**Hard requirements:**
- 64-bit only
- Targets x86_64 with AVX2 minimum, AVX-512/VNNI as a preferred path
- ARM NEON support as a secondary target
- UCI protocol compliance (the standard for chess GUIs and testing frameworks)
- A `bench` subcommand that runs a fixed search and prints node count (required by OpenBench)

**Tooling:**
- `cargo` or `cmake` build system
- A `Makefile` with an `EXE=<name>` target (OpenBench convention)
- `cutechess-cli` or `fastchess` for local testing
- An OpenBench instance (private fork) for SPRT testing once we hit Phase 4

---

## Architecture overview

```
+---------------------------------------------------------+
|                      UCI Layer                          |
+---------------------------------------------------------+
                            |
+---------------------------------------------------------+
|                    Search (PVS + ABS)                   |
|   Iterative deepening, aspiration windows, time mgmt    |
|   LMR, NMP, RFP, futility, singular ext, multi-cut...   |
+---------------------------------------------------------+
        |                |                       |
+---------------+ +-------------+ +-----------------------+
| Move Gen      | | Eval (NNUE) | | Transposition Table   |
| Magic bitbds  | | HalfKA-like | | Zobrist, 4-way bucket |
+---------------+ +-------------+ +-----------------------+
        |                |                       |
+---------------------------------------------------------+
|              Board representation (bitboards)           |
+---------------------------------------------------------+
```

Three subsystems must be airtight before anything else matters:

1. **Move generation must be perft-verified.** A single off-by-one in en-passant or castling rules ruins everything downstream and the bug will appear as "engine plays weird moves" rather than "engine fails this test".
2. **Zobrist hashing must be incremental and correct.** A broken hash means a broken TT means a broken search.
3. **Evaluation must be deterministic and reproducible.** This is what we test against during SPRT.

---

## Phase 0: Foundations (Week 1)

Goal: project skeleton, board representation, perft.

### 0.1 Repository setup
```
chess-engine/
  Cargo.toml or CMakeLists.txt
  Makefile          # OpenBench-compatible
  src/
    main.rs / main.cpp
    uci.rs
    board/
      mod.rs
      bitboard.rs
      position.rs
      zobrist.rs
      attacks.rs    # magic bitboards
    movegen/
      mod.rs
    search/
      mod.rs
    eval/
      mod.rs
    tt/
      mod.rs
  tests/
    perft_tests.rs
  nets/             # NNUE binary files
  tools/
    nnue-trainer/   # python, separate venv
```

### 0.2 Board representation (bitboards)

A `Position` struct holds:
```rust
pub struct Position {
    pieces: [[u64; 6]; 2],     // [color][piece_type] -> bitboard
    occupancy: [u64; 2],        // per color
    all: u64,
    side_to_move: Color,
    castling: u8,               // 4 bits: WK, WQ, BK, BQ
    ep_square: Option<Square>,  // en passant target
    halfmove_clock: u8,         // for 50-move rule
    fullmove: u16,
    zobrist: u64,
    pawn_zobrist: u64,          // for pawn-only hashing
    non_pawn_zobrist: [u64; 2], // for correction history later
    accumulator: Accumulator,   // NNUE state, lazy-initialized
}
```

A12-bit move encoding (fits in `u16`):
```
bits 0-5:   from square (0-63)
bits 6-11:  to square (0-63)
bits 12-15: flags (quiet, capture, ep, castle, promotion x 4, etc.)
```

Many top engines use `u16` for moves. Don't waste bits on piece info, it's recoverable from the position.

### 0.3 Magic bitboards

This is the only non-obvious piece of Phase 0. Sliding pieces (rook, bishop, queen) need O(1) attack lookups. The trick:

1. For each square, compute the **relevant blocker mask** (squares that can block this piece's rays, excluding edges).
2. For each possible blocker configuration, compute the actual attack bitboard.
3. Find a **magic number** `M` per square such that `((blockers & mask) * M) >> shift` produces a unique index into a precomputed attack table.

```rust
// For rook on square `sq`:
fn rook_attacks(sq: Square, occupancy: u64) -> u64 {
    let entry = &ROOK_MAGICS[sq as usize];
    let blockers = occupancy & entry.mask;
    let index = ((blockers.wrapping_mul(entry.magic)) >> entry.shift) as usize;
    entry.attacks[index]
}
```

Use Pradyumna Kannan's published magics or generate your own (takes ~5 seconds at startup with a trial-and-error loop). PEXT bitboards are an alternative on BMI2 CPUs (Intel Haswell+, recent AMD) and are slightly faster on hot loops, but magics are universal. Implement magic bitboards first, add a PEXT path later behind a feature flag.

### 0.4 Zobrist hashing

```rust
static ZOBRIST_PIECES: [[[u64; 64]; 6]; 2]; // [color][piece][square]
static ZOBRIST_CASTLING: [u64; 16];          // 16 castling-right combos
static ZOBRIST_EP: [u64; 8];                 // 8 files
static ZOBRIST_SIDE: u64;                    // XOR'd when black to move
```

Generate these once at startup using a known PRNG (xorshift64 or splitmix64 with a fixed seed). **Update incrementally** on every make/unmake move. Never recompute from scratch except as a debug assertion.

For correction histories later, also maintain:
- `pawn_zobrist` (only pawns and kings, used for pawn-structure correction)
- `non_pawn_zobrist[color]` (everything except that side's pawns, used for piece-correction)
- `minor_zobrist`, `major_zobrist` (variants used by some engines)

### 0.5 Move generation

Implement **legal** move generation (not pseudo-legal). It's slightly more complex but pays off in search by skipping the "is king attacked after this move" check. The technique:

1. Compute pinned pieces and check mask up front.
2. If in double check, only king moves are legal.
3. If in single check, only king moves, captures of the checker, or blocks are legal.
4. Pinned pieces can only move along the pin ray.

This is the standard "Pradu-style" or "Surge-style" legal generator.

### 0.6 Perft verification (mandatory gate)

`perft(depth)` counts leaf nodes at exactly `depth` plies. Run against the canonical test positions:

```
startpos                       perft(6) = 119,060,324
Kiwipete                       perft(5) = 193,690,690
Position 3 (rook endgame)      perft(7) = 178,633,661
Position 4 (promotion)         perft(6) = 706,045,033
Position 5 (en passant edge)   perft(5) = 89,941,194
Position 6 (Steven Edwards)    perft(5) = 164,075,551
```

**Do not advance to Phase 1 until all six positions match exactly.** A single discrepancy means a movegen bug. Common culprits: en-passant illegal because it exposes king, castling through attacked squares, double pawn push setting ep square incorrectly.

Target perft NPS: **>200M nodes/sec** for a release build on a modern CPU. If you're under 100M nps something is wrong with how you're allocating moves (avoid `Vec`, use a stack-allocated `[Move; 256]`).

---

## Phase 1: Minimal viable engine (Week 2-3)

Goal: a working UCI engine that plays legal chess at roughly 2400 Elo using only piece-square tables and basic alpha-beta.

### 1.1 Evaluation (handcrafted, temporary)

Piece values (centipawns):
```
P=100, N=320, B=330, R=500, Q=900, K=20000
```

Add tapered piece-square tables (PSQTs). Tapered means you have a midgame and endgame value per (piece, square) and interpolate by game phase:

```
phase = (knights * 1 + bishops * 1 + rooks * 2 + queens * 4)  // out of 24
mg_score = score from midgame PSQTs
eg_score = score from endgame PSQTs
eval = (mg_score * phase + eg_score * (24 - phase)) / 24
```

Use the PeSTO PSQTs (publicly available, well tuned). This alone with reasonable search gets you to ~2500 Elo.

### 1.2 Search: negamax with alpha-beta

```
function negamax(pos, depth, alpha, beta):
    if depth == 0:
        return quiescence(pos, alpha, beta)
    best = -INFINITY
    for move in generate_moves(pos):
        make(move)
        score = -negamax(pos, depth - 1, -beta, -alpha)
        unmake(move)
        if score >= beta:
            return score    // fail-soft beta cutoff
        if score > best:
            best = score
            if score > alpha:
                alpha = score
    return best
```

### 1.3 Quiescence search

Only search captures (and promotions, and check-evasions if in check) until the position is "quiet":

```
function quiescence(pos, alpha, beta):
    stand_pat = eval(pos)
    if stand_pat >= beta: return stand_pat
    if stand_pat > alpha: alpha = stand_pat
    for capture in generate_captures(pos):
        if see(capture) < 0: continue  // skip losing captures (Phase 2)
        make(capture)
        score = -quiescence(pos, -beta, -alpha)
        unmake(capture)
        if score >= beta: return score
        if score > alpha: alpha = score
    return alpha
```

### 1.4 Iterative deepening + UCI

```
for depth in 1..MAX_DEPTH:
    score = negamax(root, depth, -INFINITY, INFINITY)
    print "info depth {depth} score cp {score} pv {pv}"
    if time_up(): break
```

### 1.5 UCI implementation

Required commands: `uci`, `isready`, `ucinewgame`, `position [fen|startpos] [moves ...]`, `go [wtime|btime|winc|binc|movetime|depth|nodes|infinite]`, `stop`, `quit`, `setoption name <X> value <V>`, `bench`.

The `bench` subcommand is critical for OpenBench. It must run a fixed list of positions to a fixed depth and print the total node count. Stockfish's bench positions are a fine starting set.

### Gate to Phase 2
- Engine plays full games via cutechess-cli without crashing or making illegal moves.
- Beats a random-mover 1000-0-0.
- Scores at least 100 Elo above a known weak baseline (TSCP, Sunfish).

---

## Phase 2: Search foundations (Week 3-5)

Each item below is an independent SPRT-testable change. The Elo gains are empirical, taken from documented results in the chess engine community. Test each one with `[0, 5]` Elo bounds at 8.0+0.08 time control, ~1000-5000 games per change.

### 2.1 Transposition table (+130 Elo)

```rust
#[repr(C)]
struct TTEntry {
    key: u16,        // top 16 bits of zobrist for verification
    move: u16,       // best move from this position
    score: i16,      // adjusted for mate distance
    eval: i16,       // static eval (used to skip re-evaluating)
    depth: u8,       // search depth this entry was stored at
    bound_age: u8,   // bound type (2 bits) + age (6 bits)
}
// 10 bytes per entry, pack into 32-byte clusters (4-way set-associative)
```

Replacement policy: prefer to replace entries with lower (depth + age*4). Always store exact bounds. On a TT hit during search, return the score immediately if depth is sufficient AND the bound type allows the cutoff.

### 2.2 Move ordering (compounds with every other technique)

In order of priority at each node:
1. **TT move** (the move stored in the transposition table for this position)
2. **Good captures** sorted by MVV-LVA, filtered by SEE >= 0
3. **Promotions**
4. **Killer moves** (two quiet moves per ply that caused beta cutoffs)
5. **Counter-move heuristic** (move that historically refuted the parent's move)
6. **Quiet moves sorted by continuation history** (1-ply, 2-ply, 4-ply, 6-ply)
7. **Bad captures** (SEE < 0)

Compute scores once, then use a **staged move generator** that yields TT move, then captures, then quiets without ever materializing the full list. This is faster than sort-then-iterate.

### 2.3 Static Exchange Evaluation (SEE)

Cheap simulation of an exchange on a square, used both in move ordering and pruning:

```
function see(move) -> int:
    gain[0] = value(captured_piece)
    occupancy = current_occupancy ^ from_bit
    attackers = attackers_to(to_sq, occupancy)
    side = ~side_to_move
    piece = piece_at(from_sq)
    d = 0
    while true:
        d += 1
        gain[d] = value(piece) - gain[d-1]
        if max(-gain[d-1], gain[d]) < 0: break
        attackers ^= least_valuable_attacker(side, occupancy)
        if no_attackers: break
        side = ~side
    // negamax the gain array back to root
    while d > 0:
        d -= 1
        gain[d] = -max(-gain[d], gain[d+1])
    return gain[0]
```

### 2.4 Killer moves (+50 Elo)

Per-ply: store the two most recent moves that caused beta cutoffs at this ply. On move ordering, try them after captures.

### 2.5 History heuristic (+50 to +100 Elo)

A `[piece][to_sq] -> i32` table. On beta cutoff for a quiet move, add a bonus. On all other quiet moves searched before the cutoff, subtract a malus. Use the "gravity" formula:

```
bonus = min(depth * depth * 16, MAX_HISTORY)
history[piece][to] += bonus - history[piece][to] * abs(bonus) / MAX_HISTORY
```

This automatically clamps to `[-MAX_HISTORY, +MAX_HISTORY]` and ages naturally.

### 2.6 Continuation history (+30 to +50 Elo)

Same as history, but indexed by `(prev_move, current_move)`. Maintain at offsets of 1, 2, 4, and 6 plies back. Used both for move ordering and for LMR reduction decisions.

### 2.7 Aspiration windows (+20 Elo)

After depth 4, instead of `negamax(depth, -INF, +INF)`:

```
delta = 10
alpha = prev_score - delta
beta = prev_score + delta
loop:
    score = negamax(depth, alpha, beta)
    if score <= alpha:
        alpha -= delta
        delta *= 2
    elif score >= beta:
        beta += delta
        delta *= 2
    else:
        break
```

### 2.8 Principal Variation Search (+30 Elo)

After the first move at each node, search subsequent moves with a null window `(alpha, alpha+1)`. If a move beats alpha, re-search with the full window:

```rust
if move_index == 0 {
    score = -negamax(pos, depth - 1, -beta, -alpha);
} else {
    score = -negamax(pos, depth - 1, -alpha - 1, -alpha);
    if score > alpha && score < beta {
        score = -negamax(pos, depth - 1, -beta, -alpha);
    }
}
```

### Phase 2 expected outcome: 2800-3000 Elo

---

## Phase 3: Search pruning (Week 5-7)

These are aggressive, lossy techniques that approximate alpha-beta. Each one is tested individually with SPRT.

### 3.1 Null Move Pruning (+50 to +100 Elo)

If giving the opponent a free move still leaves us above beta, this position is so good we can prune:

```
if !in_check && depth >= 3 && static_eval >= beta && !zugzwang_likely:
    R = 4 + depth / 4 + min((eval - beta) / 200, 3)
    make_null_move()
    score = -negamax(depth - R, -beta, -beta + 1)
    unmake_null_move()
    if score >= beta:
        return score
```

Zugzwang detection: skip NMP in pure-pawn endgames (no non-pawn material for the side to move). Also skip in PV nodes when depth is high.

### 3.2 Reverse Futility Pruning / Static Null Move Pruning (+50 to +150 Elo)

If the static eval is dramatically above beta at low depth, prune:

```
if !in_check && !pv_node && depth <= 8:
    if eval - 75 * depth >= beta:
        return eval
```

### 3.3 Futility Pruning (+30 Elo)

At low depth, if static_eval + margin < alpha, skip quiet moves:

```
if !in_check && depth <= 8 && !pv_node:
    futility_margin = 150 * depth
    if static_eval + futility_margin <= alpha:
        skip_quiet_moves = true
```

### 3.4 Late Move Reductions (+100 to +200 Elo, the single biggest pruning gain)

After searching the first few moves at full depth, reduce subsequent moves. The reduction formula varies by engine, but a good starting point:

```
R = 0.7 + ln(depth) * ln(move_count) / 2.25
// For quiet moves, scale by:
//   +1 if not improving
//   -1 if PV node
//   -1 if killer or counter
//   -history[piece][to] / 8192
R = clamp(R, 0, depth - 1)
```

If the reduced search returns score > alpha, re-search at full depth. If the reduced search returns much higher than expected, extend the re-search by 1.

This is the most important single technique in modern engines. Tune the constants via SPSA later.

### 3.5 Late Move Pruning / Move Count Pruning (+20 Elo)

At low depth, after searching N moves, prune the remaining quiet moves entirely:

```
if depth <= 5 && !pv_node && move_count > 3 + depth * depth:
    skip_remaining_quiets = true
```

### 3.6 SEE Pruning in main search (+15 Elo)

Skip moves with very bad SEE at shallow depth:

```
if depth <= 8 && !pv_node:
    if see(move) < -20 * depth * depth:   // for quiets
        skip
    if see(move) < -100 * depth:           // for captures
        skip
```

### 3.7 Singular Extensions (+30 to +60 Elo)

If the TT move appears to be the only good move (a singular search at reduced depth fails low), extend the TT move's search by 1 ply:

```
if depth >= 8 && tt_hit && tt_depth >= depth - 3 && tt_bound != UPPER:
    singular_beta = tt_score - 3 * depth
    singular_depth = (depth - 1) / 2
    // search all moves except the TT move with window [singular_beta - 1, singular_beta]
    excluded_move = tt_move
    score = negamax(pos, singular_depth, singular_beta - 1, singular_beta)
    excluded_move = None
    if score < singular_beta:
        extension = 1                   // singular: extend
        if score < singular_beta - 50:
            extension = 2               // double extension
    elif singular_beta >= beta:
        return singular_beta             // multi-cut: prune entire node
```

### 3.8 Check Extensions (+30 Elo)

Trivial: `if gives_check { extension += 1 }`. Cap total extensions per branch.

### 3.9 Internal Iterative Reductions (+15 Elo)

If no TT move is available at a PV or cut node, reduce depth by 1 instead of doing a full IID search:

```
if depth >= 4 && tt_move.is_none() && (pv_node || cut_node):
    depth -= 1
```

### 3.10 Probcut (+20 Elo)

At higher depth, if a capture appears to fail high with high probability, verify with a reduced search and trust it:

```
if depth >= 5 && !pv_node && abs(beta) < MATE_BOUND:
    probcut_beta = beta + 200
    for capture in captures_with_see_above(probcut_beta - static_eval):
        score = -quiescence(pos, -probcut_beta, -probcut_beta + 1)
        if score >= probcut_beta:
            score = -negamax(pos, depth - 4, -probcut_beta, -probcut_beta + 1)
            if score >= probcut_beta:
                return score
```

### Phase 3 expected outcome: 3100-3200 Elo with handcrafted eval

---

## Phase 4: NNUE evaluation (Week 7-12, the biggest single jump)

This is the most complex phase and where you'll spend the most time. NNUE provides **400-500 Elo over a tuned handcrafted eval**, but a bad NNUE will lose Elo. Build it carefully.

### 4.1 Architecture: HalfKA-like with output buckets

Standard modern small-engine architecture (768 -> 1024)x2 -> 1x8:

```
Input: HalfKA_v2-style features (~45,000 indices, ~30 active per position)
  For each (king_square, piece, piece_square) combination, one input bit.
  Two perspectives: side-to-move's view, opponent's view.

Layer 1 (Feature Transformer):
  768 inputs (or 40,960 with HalfKA) -> 1024 hidden neurons per perspective
  Activation: Squared Clipped ReLU (SCReLU)
  Stored as i16 weights, output i16

Layer 2:
  2048 inputs (concatenated perspectives) -> 1 output
  But: 8 output buckets selected by (piece_count - 1) / 4
  i.e. the network has 8 separate output heads, one for each game phase

Final scaling:
  output / FT_SCALE / NETWORK_SCALE -> centipawn score
```

For a first NNUE, simpler works: `768 -> 512 perspective -> 1`, no output buckets, ~256 Elo gain over PSQTs.

### 4.2 Incremental updates (the "Efficiently Updatable" part)

When making a move, only 2-4 features change (the moving piece, the captured piece, possibly the rook in castling). Instead of recomputing the entire first layer:

```rust
fn make_move_accumulator(acc: &mut Accumulator, move: Move, pos: &Position) {
    // Removed features
    for (color, piece, square) in features_removed(move, pos) {
        let idx = feature_index(color, piece, square, our_king, their_king);
        for i in 0..HIDDEN_SIZE {
            acc.values[color][i] -= weights[idx][i];
        }
    }
    // Added features
    for (color, piece, square) in features_added(move, pos) {
        let idx = feature_index(color, piece, square, our_king, their_king);
        for i in 0..HIDDEN_SIZE {
            acc.values[color][i] += weights[idx][i];
        }
    }
}
```

This is the entire reason NNUE is fast. SIMD this inner loop (AVX2 = 16 i16s per register, AVX-512 = 32). You should hit 50-100 million accumulator updates per second.

**Critical:** when the king moves, the feature indices for the whole side change (because they're indexed by king square). This requires a full refresh. To avoid this being a bottleneck, use **finny tables / accumulator caches**: maintain cached accumulators keyed by king square so common king positions don't re-pay the cost.

### 4.3 Quantization

Training is done in float32 in PyTorch. Inference must be in int8/int16:

```
Feature Transformer weights:  i16, scale 255 (so float w -> round(w * 255))
Feature Transformer biases:   i16, scale 255
Hidden layer weights:         i8, scale 64
Hidden layer biases:          i32, scale 255 * 64

SCReLU activation: clamp(x, 0, 255)^2 / 255
```

The exact scales depend on your network. The key constraint: weights and biases must fit in the target precision after scaling, or you get clipping artifacts and Elo loss.

### 4.4 SIMD inference

For the feature transformer accumulator update, use `vpaddw` (AVX2) or `vpaddw` (AVX-512). For the output layer dot product, use `vpdpbusd` (AVX-512 VNNI) which does an int8 dot product in one instruction. On CPUs without VNNI, fall back to `vpmaddubsw + vpmaddwd + vpaddd` sequence.

Stockfish's `simd.h` is a good reference (BSD-style permissive for non-commercial study, but write your own to avoid GPL).

### 4.5 Training pipeline (PyTorch)

```
tools/nnue-trainer/
  train.py
  model.py          # nn.Module with feature transformer + perspective layers
  dataset.py        # bullet-format or marlinflow-format position loader
  serialize.py      # exports trained weights to engine binary format
```

Training data: generate ~1 billion positions via self-play of your handcrafted-eval engine at depth 8-10. Each position gets:
- The FEN/position
- A search score in centipawns (from your engine, not from Stockfish, to avoid plagiarism issues)
- A game result (W/D/L)

Train with a loss that blends search eval and game result:
```
loss = lambda * mse(pred, score) + (1 - lambda) * cross_entropy(sigmoid(pred), result)
lambda = 0.7 typically
```

Use the [bullet trainer](https://github.com/jw1912/bullet) for Rust engines, or the official `nnue-pytorch` for C++ engines.

**First-net target: 200 Elo over handcrafted eval.** This is a low bar and very achievable. Subsequent nets gain 20-50 Elo per retrain as you add more data and refine architecture.

### 4.6 Two-net hybrid (Stockfish-style, optional)

After you have a working big net, train a small one (128 hidden) for positions with large material imbalance. At eval time:

```rust
if material_imbalance > 962 {
    use_small_net()
} else {
    use_big_net()
}
```

This is ~10-15 Elo and ~5% speedup on average. Skip until late optimization.

### Phase 4 expected outcome: 3300-3500 Elo

---

## Phase 5: Parallelism (Week 12-14)

### 5.1 Lazy SMP

The simplest effective parallelization. All threads search the same root with the shared TT, but with slight perturbations (different depths, slightly different aspiration windows). The shared TT means deeper threads benefit from shallower threads' work.

```rust
fn search(threads: usize) {
    let shared_tt = Arc::new(TT::new(hash_mb));
    let stop = Arc::new(AtomicBool::new(false));
    let scope = thread::scope(|s| {
        for tid in 0..threads {
            s.spawn(|| thread_search(tid, shared_tt.clone(), stop.clone()));
        }
    });
}
```

Expected gain: ~32 Elo at 4 threads, ~50 Elo at 16 threads. Diminishing returns are real, threading is memory-bound at high core counts.

**TT must be lockless.** Use a relaxed atomic compare-and-swap or just accept torn writes with proper key validation (top 16 bits of zobrist verify the entry on read).

### 5.2 Helper threads with skipped depths

Standard trick: even-numbered threads skip every 4th depth, odd threads search normally. Forces thread divergence without explicit coordination.

---

## Phase 6: Time management, polish, testing infrastructure (Week 14-16)

### 6.1 Time management

Allocate per-move time as:
```
soft_limit = remaining_time / 20 + increment * 3 / 4
hard_limit = remaining_time / 5 + increment
```

Stop iterative deepening if:
- Hard limit exceeded
- Soft limit exceeded AND best move has been stable for 4+ iterations
- Best move score has dropped by 50+ cp from previous iteration (extend time)

Use **node-based time management** within a depth: if 50% of nodes have been spent on the best move, we're confident and can stop early.

### 6.2 SPRT testing setup

Spin up a private OpenBench instance (it's open source: https://github.com/AndyGrant/OpenBench). Every change passes through SPRT:

```
Gainer test:  H0 = 0 Elo, H1 = 5 Elo, alpha = beta = 0.05
              LLR bounds: [-2.94, +2.94]
              TC: 8.0+0.08 (8 seconds + 80ms increment)
              ~2000-5000 games to conclude

Non-regression: H0 = -2.50, H1 = 0.50
                Used for refactors and cleanups
```

Run on a private cloud VM (Hetzner CPX51 or equivalent, ~$50/mo) to get 10K games/day. Don't trust any patch without SPRT.

### 6.3 SPSA tuning

Once the engine is stable, tune every numerical constant (LMR factors, NMP reduction, futility margins, history bonuses, aspiration window size) via SPSA. OpenBench supports this natively. Expected gain from a full tune: 30-50 Elo.

---

## Phase 7: Endgames

### 7.1 Syzygy tablebases (+10-20 Elo)

3-4-5 piece tablebases: 1 GB, trivial to embed. Download from Lichess's mirror.
6-piece: 150 GB total (WDL: 68 GB, DTZ: 82 GB). Optional, hosted access works.
7-piece: 16.7 TB, only useful for analysis, not real games.

Implement WDL probing during search (cuts off when result is known). Implement DTZ probing at the root to ensure winning play under the 50-move rule. The official Syzygy probing code is BSD-licensed and portable.

### 7.2 Endgame-specific knowledge

Even with tablebases, some heuristics help in unbalanced positions outside table coverage:
- KPK, KBNK, KRPKR are common; even a small handcrafted bonus for correct king centralization helps.
- Recognize fortresses and avoid evaluating them as winning.

---

## Phase 8: Stretch goals (where novelty might live)

By this point you have a 3300-3500 Elo engine. To go further, the field is open. Promising directions:

### 8.1 Architecture experimentation

- **Larger NNUE with output buckets per king bucket** (mirrored). Stockfish uses king buckets so positions with the king in different zones get different evaluation, capturing king-safety implicitly.
- **Threat-aware features**: adding "this square is attacked by piece X" features (Stockfish's `FullThreats` feature set). +10-20 Elo.
- **Categorical value heads** (Lc0 BT4-style): instead of predicting a centipawn scalar, predict a distribution over WDL. Then convert to scalar at eval time. This regularizes training.
- **Smolgen-like dynamic attention** (Lc0 transformer idea, but you'd be doing NNUE-style not MCTS): theoretically lets shallow networks model long-range dependencies. No one has tried this in NNUE; if it works it's novel.

### 8.2 Search improvements

- **Correction histories** (pawn-corrhist, non-pawn-corrhist, material-corrhist, continuation-corrhist): adjust the static eval based on historical search-vs-eval discrepancy. Stockfish gains ~20 Elo from these. Top engines have added variants steadily.
- **Better singular extensions logic** with more accurate margin computation.
- **Multicut with explicit verification** at high depth.

### 8.3 The genuinely speculative ideas

These are not proven to work. They're the kind of thing that, if you can make them work, would be your novel contribution:

- **Cluster-of-experts NNUE**: gate between many small networks based on position type, instead of one big network. Could massively increase model capacity per inference cost.
- **Search-aware NNUE training**: train the network using gradients that flow through the search tree (differentiable search). Lc0 tried versions of this with mixed results.
- **Learned reduction policies**: instead of hand-tuned LMR formulas, train a tiny network to predict optimal reduction. Has been tried in research papers, never deployed in a top engine.
- **Hybrid MCTS + alpha-beta**: run alpha-beta as the inner loop of MCTS rollouts. Researched, never made strong.
- **Position-specific opening books from your own engine's analysis**: trivial to implement, modest Elo, mostly useful for tournament play.

---

## Mathematical reference

### Elo difference and expected score

$$E = \frac{1}{1 + 10^{-D/400}}$$

Where $D$ is the Elo difference and $E$ is the expected score (0 to 1).

100 Elo difference = 64% expected score
200 Elo difference = 76% expected score
400 Elo difference = 91% expected score

### Branching factor with alpha-beta

Naive minimax: $b^d$ nodes for branching factor $b$ and depth $d$.

Perfect alpha-beta (best move first always): $b^{d/2}$ nodes. Effective branching factor $\sqrt{b}$.

In chess with $b \approx 35$: minimax at depth 10 = $35^{10} \approx 2.7 \times 10^{15}$ nodes. Alpha-beta at depth 10 = $35^5 \approx 5 \times 10^7$ nodes. Stockfish hits depth 30+ on modern hardware because aggressive pruning brings effective $b$ down to ~1.5-2.

### LMR formula derivation

Most engines use:
$$R(d, m) = c_0 + \frac{\ln(d) \cdot \ln(m)}{c_1}$$

Where $d$ is current depth, $m$ is move number. Typical values $c_0 \in [0.5, 1.0]$ and $c_1 \in [2.0, 3.5]$. The log-log shape captures: reduce more at higher depth (we can afford it) and reduce more for later moves (they're less likely to be best).

### SPRT bounds

$$LLR = \sum_i \ln \frac{P(x_i | H_1)}{P(x_i | H_0)}$$

For Elo testing, $x_i$ is the result of game $i$ (loss=0, draw=0.5, win=1). Under $H_0$ vs $H_1$ representing the two Elo hypotheses, with $\alpha = \beta = 0.05$:

Accept $H_1$ when $LLR > 2.94$. Accept $H_0$ when $LLR < -2.94$. Otherwise keep playing.

A 5 Elo improvement at 8s+0.08s typically takes 2000-5000 games to resolve. A 1 Elo improvement can take 50,000+. This is why testing infrastructure matters so much.

### Quantization error

For an NNUE with $L$ layers and per-layer quantization error $\epsilon$:

$$E_{total} \approx \epsilon \sqrt{L}$$

For 4 layers at int8 (max $\epsilon \approx 1/127$): total error around 1.6%. This is why shallow networks are critical for NNUE: deep networks accumulate too much quantization error.

---

## Testing rituals (non-negotiable)

1. **Every code change runs `make bench` first.** Compare node count to previous commit. If it changed but you didn't intend it to, you have a bug.
2. **Perft suite runs in CI on every PR.** Catches movegen regressions.
3. **No optimization or feature lands without SPRT verification.** "I think this is faster" is meaningless; show me the LLR.
4. **Maintain a `bench.txt` of known-good (FEN, depth, expected_nodes, expected_eval) tuples.** Run on every release build.
5. **Diff games against the previous version.** If you gained Elo but lost positions you used to win, your eval is shifting in ways you don't understand.

---

## How to work with Claude Code on this

When I (the human) come to you (Claude Code) for each phase:

1. **Read this entire document first.** Don't pattern-match off the section headers.
2. **Confirm the current phase before writing code.** Ask which phase we're in and what's already implemented.
3. **For each new feature, propose:** the implementation sketch, the test strategy, the expected Elo gain (or "no Elo, refactor only"), and any risks.
4. **Write code that compiles and runs. ** No pseudocode in source files. If you reference a function, define it.
5. **Always include a perft sanity check** when modifying movegen.
6. **When uncertain about a constant** (LMR coefficient, futility margin, etc.), use the value cited in this document and mark it `// SPSA tunable` so we tune later.
7. **Don't suggest GPL-licensed code copies.** Stockfish is GPLv3. We're writing original code. Reference algorithms, not source.
8. **Bench every commit.** The Makefile must support `make bench`.

When I say "let's do Phase X", load the relevant section, propose the order of implementation within that phase, and start with the smallest testable unit.

---

## Initial action

Once I confirm this plan, your first job is **Phase 0.1 through 0.6**: scaffold the project, implement the board representation, magic bitboards, Zobrist hashing, legal move generation, and perft verification.

Do not write a search function, an evaluation function, or a UCI handler until perft passes all six standard test positions. The temptation to "just get it playing" will be strong. Resist it. A buggy movegen has wasted more chess-engine-author hours than every other category of bug combined.

When perft passes, we'll review together and move to Phase 1.

Go.
