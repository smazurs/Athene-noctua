/// Magic bitboard attack tables for sliding pieces plus
/// pre-computed tables for knights, kings, and pawns.

use super::bitboard::*;
use super::types::Square;

// --------------------------------------------------------------------------
// Leaper tables (knight, king, pawn)
// --------------------------------------------------------------------------

static mut KNIGHT_ATTACKS: [Bitboard; 64] = [0; 64];
static mut KING_ATTACKS: [Bitboard; 64] = [0; 64];
static mut PAWN_ATTACKS: [[Bitboard; 64]; 2] = [[0; 64]; 2];

pub fn knight_attacks(sq: Square) -> Bitboard {
    unsafe { KNIGHT_ATTACKS[sq as usize] }
}

pub fn king_attacks(sq: Square) -> Bitboard {
    unsafe { KING_ATTACKS[sq as usize] }
}

/// color 0 = White, 1 = Black
pub fn pawn_attacks(sq: Square, color: usize) -> Bitboard {
    unsafe { PAWN_ATTACKS[color][sq as usize] }
}

fn init_knight_attacks() {
    for sq in 0u32..64 {
        unsafe { KNIGHT_ATTACKS[sq as usize] = compute_knight(sq); }
    }
}

fn compute_knight(sq: u32) -> Bitboard {
    let b = bit(sq);
    ((b & NOT_FILE_AB) >> 10)
        | ((b & NOT_FILE_A) >> 17)
        | ((b & NOT_FILE_H) >> 15)
        | ((b & NOT_FILE_GH) >> 6)
        | ((b & NOT_FILE_AB) << 6)
        | ((b & NOT_FILE_A) << 15)
        | ((b & NOT_FILE_H) << 17)
        | ((b & NOT_FILE_GH) << 10)
}

fn compute_king(sq: u32) -> Bitboard {
    let b = bit(sq);
    north(b) | south(b) | east(b) | west(b)
        | north_east(b) | north_west(b)
        | south_east(b) | south_west(b)
}

fn init_king_attacks() {
    for sq in 0u32..64 {
        unsafe { KING_ATTACKS[sq as usize] = compute_king(sq); }
    }
}

fn init_pawn_attacks() {
    for sq in 0u32..64 {
        let b = bit(sq);
        unsafe {
            PAWN_ATTACKS[0][sq as usize] = north_east(b) | north_west(b); // White
            PAWN_ATTACKS[1][sq as usize] = south_east(b) | south_west(b); // Black
        }
    }
}

// --------------------------------------------------------------------------
// Magic bitboards for sliding pieces
// --------------------------------------------------------------------------

struct MagicEntry {
    mask: Bitboard,
    magic: u64,
    shift: u32,
    offset: usize,
}

// Rook and bishop attack tables.
// Total size: rooks need up to 4096 entries per square, bishops up to 512.
static mut ROOK_ATTACKS: Vec<Bitboard> = Vec::new();
static mut BISHOP_ATTACKS: Vec<Bitboard> = Vec::new();

static mut ROOK_MAGICS: [MagicEntry; 64] = unsafe { std::mem::zeroed() };
static mut BISHOP_MAGICS: [MagicEntry; 64] = unsafe { std::mem::zeroed() };

/// Compute sliding attacks along rays given occupancy (slow, used during init).
fn sliding_attacks(sq: u32, occ: Bitboard, deltas: &[(i32, i32)]) -> Bitboard {
    let mut result = 0u64;
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    for &(df, dr) in deltas {
        let (mut f, mut r) = (file + df, rank + dr);
        while f >= 0 && f < 8 && r >= 0 && r < 8 {
            let s = (r * 8 + f) as u32;
            result |= bit(s);
            if occ & bit(s) != 0 {
                break;
            }
            f += df;
            r += dr;
        }
    }
    result
}

const ROOK_DELTAS: &[(i32, i32)] = &[(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DELTAS: &[(i32, i32)] = &[(1, 1), (-1, 1), (1, -1), (-1, -1)];

/// Relevant blocker mask for rook on sq (exclude edges on each ray).
fn rook_mask(sq: u32) -> Bitboard {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    let mut mask = 0u64;
    // Horizontal ray (exclude files A and H except for edges)
    for f in (file + 1)..7 {
        mask |= bit((rank * 8 + f) as u32);
    }
    for f in 1..file {
        mask |= bit((rank * 8 + f) as u32);
    }
    // Vertical ray
    for r in (rank + 1)..7 {
        mask |= bit((r * 8 + file) as u32);
    }
    for r in 1..rank {
        mask |= bit((r * 8 + file) as u32);
    }
    mask
}

/// Relevant blocker mask for bishop on sq (exclude edges).
fn bishop_mask(sq: u32) -> Bitboard {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    let mut mask = 0u64;
    for &(df, dr) in BISHOP_DELTAS {
        let (mut f, mut r) = (file + df, rank + dr);
        while f > 0 && f < 7 && r > 0 && r < 7 {
            mask |= bit((r * 8 + f) as u32);
            f += df;
            r += dr;
        }
    }
    mask
}

/// Known-good magic numbers for rooks (Pradyumna Kannan's table).
const ROOK_MAGIC_NUMBERS: [u64; 64] = [
    0x8a80104000800020, 0x140002000100040, 0x2801880a0017001, 0x100081001000420,
    0x200020010080420, 0x3001c0002010008, 0x8480008002000100, 0x2080088004402900,
    0x800098204000, 0x2024401000200040, 0x100802000801000, 0x120800800801000,
    0x208808088000400, 0x2802200800400, 0x2200800100020080, 0x801000060821100,
    0x80044006422000, 0x100808020004000, 0x12108a0010204200, 0x140848010000802,
    0x481828014002800, 0x8094004002004100, 0x4010040010010802, 0x20008806104,
    0x100400080208000, 0x2040002120081000, 0x21200680100081, 0x20100080080080,
    0x2000a00200410, 0x20080800400, 0x80088400100102, 0x80004600042881,
    0x4040008040800020, 0x440003000200801, 0x4200011004500, 0x188020010100100,
    0x14800401802800, 0x2080040080800200, 0x124080204001001, 0x200046502000484,
    0x480400080088020, 0x1000422010034000, 0x30200100110040, 0x100021010009,
    0x2002080100110004, 0x202008004008002, 0x20020004010100, 0x2048440040820001,
    0x101002200408200, 0x40802000401080, 0x4008142004410100, 0x2060820c0120200,
    0x1001004080100, 0x20c020080040080, 0x2935610830022400, 0x44440041009200,
    0x280001040802101, 0x2100190040002085, 0x80c0084100102001, 0x4024081001000421,
    0x20030a0244872, 0x12001008414402, 0x2006104900a0804, 0x1004081002402,
];

/// Known-good magic numbers for bishops.
const BISHOP_MAGIC_NUMBERS: [u64; 64] = [
    0x40040844404084, 0x2004208a004208, 0x10190041080202, 0x108060845042010,
    0x581104180800210, 0x2112080446200010, 0x1080820820060210, 0x3c0808410220200,
    0x4050404440404, 0x21001420088, 0x24d0080801082102, 0x1020a0a020400,
    0x40308200402, 0x4011002100800, 0x401484104104005, 0x801010402020200,
    0x400210c3880100, 0x404022024108200, 0x810018200204102, 0x4002801a02003,
    0x85040820080400, 0x810102c808880400, 0xe900410884800, 0x8002020480840102,
    0x220200865090201, 0x2010100a02021202, 0x152048408022401, 0x20080002081110,
    0x4001001021004000, 0x800040400a011002, 0xe4004081011002, 0x1c004001012080,
    0x8004200962a00220, 0x8422100208500202, 0x2000402200300c08, 0x8646020680810,
    0x80020a0200100808, 0x2010004880111000, 0x623000a080011400, 0x42008c0340209202,
    0x209188240001000, 0x400408a884001800, 0x110400a6080400, 0x1840060a44020800,
    0x90080104000041, 0x201011000808101, 0x1a2208080504f080, 0x8012020600211212,
    0x500861011240000, 0x180806108200800, 0x4000020e01040044, 0x300000261044000a,
    0x802241102020002, 0x20906061210001, 0x5a84841004010310, 0x4010801011c04,
    0xa010109502200, 0x4a02012000, 0x500201010098b028, 0x8040002811040900,
    0x28000010020204, 0x6000020202d0240, 0x8918844842082200, 0x4010011029020020,
];

/// Simple xorshift64 PRNG for magic finding.
fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Generate a sparse random candidate (few set bits = better magic candidate).
fn sparse_random(state: &mut u64) -> u64 {
    xorshift64(state) & xorshift64(state) & xorshift64(state)
}

/// Find a valid magic number for a given square and mask.
/// Uses trial-and-error with sparse random candidates.
fn find_magic(sq: u32, mask: Bitboard, deltas: &[(i32, i32)]) -> (u64, u32) {
    let bits = mask.count_ones();
    let size = 1usize << bits;
    let shift = 64 - bits;

    // Pre-compute all subsets and their attacks
    let mut subsets = vec![0u64; size];
    let mut attacks = vec![0u64; size];
    let mut subset: Bitboard = 0;
    let mut n = 0;
    loop {
        subsets[n] = subset;
        attacks[n] = sliding_attacks(sq, subset, deltas);
        n += 1;
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 { break; }
    }

    let mut used = vec![0u64; size];
    let mut rng = 0x123456789abcdefu64 ^ (sq as u64 * 0x9e3779b97f4a7c15);

    'outer: loop {
        let magic = sparse_random(&mut rng);
        // Quick sanity check: magic must spread the top mask bits
        if ((mask.wrapping_mul(magic)) >> 56).count_ones() < 6 { continue; }

        // Clear used table
        for v in &mut used { *v = !0; }

        for i in 0..n {
            let idx = (subsets[i].wrapping_mul(magic) >> shift) as usize;
            if used[idx] == !0 {
                used[idx] = attacks[i];
            } else if used[idx] != attacks[i] {
                continue 'outer; // collision
            }
        }
        return (magic, shift);
    }
}

fn init_magic_table(
    sq: u32,
    mask: Bitboard,
    magic: u64,
    shift: u32,
    deltas: &[(i32, i32)],
    attacks_vec: &mut Vec<Bitboard>,
) -> usize {
    let size = 1usize << (64 - shift);
    let offset = attacks_vec.len();
    attacks_vec.resize(offset + size, 0);

    let mut subset: Bitboard = 0;
    loop {
        let idx = ((subset.wrapping_mul(magic)) >> shift) as usize;
        attacks_vec[offset + idx] = sliding_attacks(sq, subset, deltas);
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 { break; }
    }
    offset
}

fn init_magics() {
    unsafe {
        ROOK_ATTACKS = Vec::new();
        BISHOP_ATTACKS = Vec::new();

        for sq in 0u32..64 {
            let mask = rook_mask(sq);
            // Try hardcoded magic first; fall back to search if it has collisions
            let (magic, shift) = verify_or_find_magic(sq, mask, ROOK_MAGIC_NUMBERS[sq as usize], ROOK_DELTAS);
            let off = init_magic_table(sq, mask, magic, shift, ROOK_DELTAS, &mut ROOK_ATTACKS);
            ROOK_MAGICS[sq as usize] = MagicEntry { mask, magic, shift, offset: off };
        }

        for sq in 0u32..64 {
            let mask = bishop_mask(sq);
            let (magic, shift) = verify_or_find_magic(sq, mask, BISHOP_MAGIC_NUMBERS[sq as usize], BISHOP_DELTAS);
            let off = init_magic_table(sq, mask, magic, shift, BISHOP_DELTAS, &mut BISHOP_ATTACKS);
            BISHOP_MAGICS[sq as usize] = MagicEntry { mask, magic, shift, offset: off };
        }
    }
}

/// Verify a given magic number works; if not, find a valid one.
fn verify_or_find_magic(sq: u32, mask: Bitboard, candidate: u64, deltas: &[(i32, i32)]) -> (u64, u32) {
    let bits = mask.count_ones();
    let size = 1usize << bits;
    let shift = 64 - bits;

    let mut used = vec![u64::MAX; size];
    let mut subset: Bitboard = 0;
    let mut ok = true;
    loop {
        let atk = sliding_attacks(sq, subset, deltas);
        let idx = (subset.wrapping_mul(candidate) >> shift) as usize;
        if used[idx] == u64::MAX {
            used[idx] = atk;
        } else if used[idx] != atk {
            ok = false;
            break;
        }
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 { break; }
    }

    if ok {
        (candidate, shift)
    } else {
        find_magic(sq, mask, deltas)
    }
}

#[inline(always)]
pub fn rook_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    unsafe {
        let entry = &ROOK_MAGICS[sq as usize];
        let blockers = occupancy & entry.mask;
        let idx = (blockers.wrapping_mul(entry.magic) >> entry.shift) as usize;
        ROOK_ATTACKS[entry.offset + idx]
    }
}

#[inline(always)]
pub fn bishop_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    unsafe {
        let entry = &BISHOP_MAGICS[sq as usize];
        let blockers = occupancy & entry.mask;
        let idx = (blockers.wrapping_mul(entry.magic) >> entry.shift) as usize;
        BISHOP_ATTACKS[entry.offset + idx]
    }
}

#[inline(always)]
pub fn queen_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    rook_attacks(sq, occupancy) | bishop_attacks(sq, occupancy)
}

/// Initialize all attack tables. Must be called once at startup.
pub fn init() {
    // knight table init (correct version without dead code)
    for sq in 0u32..64 {
        unsafe {
            KNIGHT_ATTACKS[sq as usize] = compute_knight(sq);
            KING_ATTACKS[sq as usize] = compute_king(sq);
            PAWN_ATTACKS[0][sq as usize] = north_east(bit(sq)) | north_west(bit(sq));
            PAWN_ATTACKS[1][sq as usize] = south_east(bit(sq)) | south_west(bit(sq));
        }
    }
    init_magics();
}

/// All pieces attacking a square given the current occupancy.
pub fn attackers_to(sq: Square, occ: Bitboard, all_pieces: &[[Bitboard; 6]; 2]) -> Bitboard {
    let rq = all_pieces[0][3] | all_pieces[1][3] | all_pieces[0][4] | all_pieces[1][4];
    let bq = all_pieces[0][2] | all_pieces[1][2] | all_pieces[0][4] | all_pieces[1][4];
    let mut result = 0u64;
    result |= pawn_attacks(sq, 0) & all_pieces[1][0]; // Black pawns attacking this sq from white's perspective
    result |= pawn_attacks(sq, 1) & all_pieces[0][0]; // White pawns attacking from black's perspective
    result |= knight_attacks(sq) & (all_pieces[0][1] | all_pieces[1][1]);
    result |= king_attacks(sq) & (all_pieces[0][5] | all_pieces[1][5]);
    result |= rook_attacks(sq, occ) & rq;
    result |= bishop_attacks(sq, occ) & bq;
    result
}
