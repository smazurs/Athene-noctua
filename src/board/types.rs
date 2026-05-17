/// Square indices: A1=0, B1=1, ..., H8=63
pub type Square = u32;

pub mod squares {
    use super::Square;
    pub const A1: Square = 0;
    pub const B1: Square = 1;
    pub const C1: Square = 2;
    pub const D1: Square = 3;
    pub const E1: Square = 4;
    pub const F1: Square = 5;
    pub const G1: Square = 6;
    pub const H1: Square = 7;
    pub const A8: Square = 56;
    pub const B8: Square = 57;
    pub const C8: Square = 58;
    pub const D8: Square = 59;
    pub const E8: Square = 60;
    pub const F8: Square = 61;
    pub const G8: Square = 62;
    pub const H8: Square = 63;
    pub const NONE: Square = 64;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline(always)]
    pub fn flip(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Piece {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

impl Piece {
    pub fn from_u8(v: u8) -> Option<Piece> {
        match v {
            0 => Some(Piece::Pawn),
            1 => Some(Piece::Knight),
            2 => Some(Piece::Bishop),
            3 => Some(Piece::Rook),
            4 => Some(Piece::Queen),
            5 => Some(Piece::King),
            _ => None,
        }
    }

    pub fn from_index(i: usize) -> Piece {
        match i {
            0 => Piece::Pawn,
            1 => Piece::Knight,
            2 => Piece::Bishop,
            3 => Piece::Rook,
            4 => Piece::Queen,
            _ => Piece::King,
        }
    }
}

/// Castling rights bitmask: WK=1, WQ=2, BK=4, BQ=8.
pub const CASTLE_WK: u8 = 1;
pub const CASTLE_WQ: u8 = 2;
pub const CASTLE_BK: u8 = 4;
pub const CASTLE_BQ: u8 = 8;

pub fn rank_of(sq: Square) -> u32 {
    sq >> 3
}

pub fn file_of(sq: Square) -> u32 {
    sq & 7
}

pub fn make_square(rank: u32, file: u32) -> Square {
    rank * 8 + file
}

pub fn sq_name(sq: Square) -> String {
    let file = b'a' + file_of(sq) as u8;
    let rank = b'1' + rank_of(sq) as u8;
    format!("{}{}", file as char, rank as char)
}

pub fn parse_sq(s: &str) -> Option<Square> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let file = bytes[0].wrapping_sub(b'a') as u32;
    let rank = bytes[1].wrapping_sub(b'1') as u32;
    if file > 7 || rank > 7 {
        return None;
    }
    Some(make_square(rank, file))
}
