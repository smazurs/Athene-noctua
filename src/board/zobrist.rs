use super::types::{Color, Piece, Square};

pub struct ZobristKeys {
    pub pieces: [[[u64; 64]; 6]; 2], // [color][piece][square]
    pub castling: [u64; 16],
    pub ep: [u64; 8], // by file
    pub side: u64,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

impl ZobristKeys {
    pub fn new() -> Self {
        let mut seed: u64 = 0xDEADBEEFCAFEBABE;
        let mut pieces = [[[0u64; 64]; 6]; 2];
        let mut castling = [0u64; 16];
        let mut ep = [0u64; 8];

        for c in 0..2 {
            for p in 0..6 {
                for s in 0..64 {
                    pieces[c][p][s] = splitmix64(&mut seed);
                }
            }
        }
        for i in 0..16 {
            castling[i] = splitmix64(&mut seed);
        }
        for i in 0..8 {
            ep[i] = splitmix64(&mut seed);
        }
        let side = splitmix64(&mut seed);
        ZobristKeys { pieces, castling, ep, side }
    }

    #[inline(always)]
    pub fn piece_key(&self, color: Color, piece: Piece, sq: Square) -> u64 {
        self.pieces[color as usize][piece as usize][sq as usize]
    }

    #[inline(always)]
    pub fn castling_key(&self, rights: u8) -> u64 {
        self.castling[rights as usize]
    }

    #[inline(always)]
    pub fn ep_key(&self, file: u32) -> u64 {
        self.ep[file as usize]
    }
}

// Global lazy-initialized keys
use std::sync::OnceLock;
static KEYS: OnceLock<ZobristKeys> = OnceLock::new();

pub fn keys() -> &'static ZobristKeys {
    KEYS.get_or_init(ZobristKeys::new)
}
