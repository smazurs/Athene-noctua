use super::attacks::{
    attackers_to, bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks,
    rook_attacks,
};
use super::bitboard::*;
use super::moves::*;
use super::types::*;
use super::zobrist::keys;

/// History state for unmake_move.
#[derive(Clone, Copy)]
pub struct Irreversible {
    pub castling: u8,
    pub ep_square: u32, // squares::NONE if absent
    pub halfmove_clock: u8,
    pub zobrist: u64,
    pub captured_piece: Option<Piece>,
}

pub struct Position {
    /// [color][piece] bitboards
    pub pieces: [[Bitboard; 6]; 2],
    pub occupancy: [Bitboard; 2],
    pub all: Bitboard,
    pub side: Color,
    pub castling: u8,
    pub ep_square: u32, // squares::NONE if absent
    pub halfmove_clock: u8,
    pub fullmove: u16,
    pub zobrist: u64,
    pub pawn_zobrist: u64,
    /// Piece on each square, [color*6 + piece] or 0xFF for empty.
    /// Encoded as color << 3 | piece.
    pub mailbox: [u8; 64],
    pub history: Vec<Irreversible>,
}

const EMPTY_MAILBOX: u8 = 0xFF;

impl Position {
    pub fn new() -> Self {
        Position {
            pieces: [[0; 6]; 2],
            occupancy: [0; 2],
            all: 0,
            side: Color::White,
            castling: 0,
            ep_square: squares::NONE,
            halfmove_clock: 0,
            fullmove: 1,
            zobrist: 0,
            pawn_zobrist: 0,
            mailbox: [EMPTY_MAILBOX; 64],
            history: Vec::with_capacity(512),
        }
    }

    /// Place a piece on the board, updating all data structures.
    fn place_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        let bb = bit(sq);
        self.pieces[color as usize][piece as usize] |= bb;
        self.occupancy[color as usize] |= bb;
        self.all |= bb;
        self.mailbox[sq as usize] = (color as u8) << 3 | (piece as u8);
        self.zobrist ^= keys().piece_key(color, piece, sq);
        if piece == Piece::Pawn || piece == Piece::King {
            self.pawn_zobrist ^= keys().piece_key(color, piece, sq);
        }
    }

    /// Remove a piece from the board.
    fn remove_piece(&mut self, color: Color, piece: Piece, sq: Square) {
        let bb = bit(sq);
        self.pieces[color as usize][piece as usize] ^= bb;
        self.occupancy[color as usize] ^= bb;
        self.all ^= bb;
        self.mailbox[sq as usize] = EMPTY_MAILBOX;
        self.zobrist ^= keys().piece_key(color, piece, sq);
        if piece == Piece::Pawn || piece == Piece::King {
            self.pawn_zobrist ^= keys().piece_key(color, piece, sq);
        }
    }

    fn move_piece(&mut self, color: Color, piece: Piece, from: Square, to: Square) {
        self.remove_piece(color, piece, from);
        self.place_piece(color, piece, to);
    }

    pub fn piece_at(&self, sq: Square) -> Option<(Color, Piece)> {
        let raw = self.mailbox[sq as usize];
        if raw == EMPTY_MAILBOX {
            return None;
        }
        let color = if raw >> 3 == 0 { Color::White } else { Color::Black };
        let piece = Piece::from_u8(raw & 7)?;
        Some((color, piece))
    }

    pub fn king_sq(&self, color: Color) -> Square {
        lsb(self.pieces[color as usize][Piece::King as usize])
    }

    pub fn in_check(&self) -> bool {
        let ksq = self.king_sq(self.side);
        self.is_attacked(ksq, self.side.flip())
    }

    /// Is `sq` attacked by `attacker`?
    pub fn is_attacked(&self, sq: Square, attacker: Color) -> bool {
        let ai = attacker as usize;
        let occ = self.all;
        if pawn_attacks(sq, attacker.flip() as usize) & self.pieces[ai][Piece::Pawn as usize] != 0
        {
            return true;
        }
        if knight_attacks(sq) & self.pieces[ai][Piece::Knight as usize] != 0 {
            return true;
        }
        if king_attacks(sq) & self.pieces[ai][Piece::King as usize] != 0 {
            return true;
        }
        if bishop_attacks(sq, occ)
            & (self.pieces[ai][Piece::Bishop as usize]
                | self.pieces[ai][Piece::Queen as usize])
            != 0
        {
            return true;
        }
        if rook_attacks(sq, occ)
            & (self.pieces[ai][Piece::Rook as usize]
                | self.pieces[ai][Piece::Queen as usize])
            != 0
        {
            return true;
        }
        false
    }

    /// Make a null move (pass the turn). Only valid when not in check.
    pub fn make_null_move(&mut self) {
        let irr = Irreversible {
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove_clock: self.halfmove_clock,
            zobrist: self.zobrist,
            captured_piece: None,
        };
        if self.ep_square != squares::NONE {
            self.zobrist ^= keys().ep_key(file_of(self.ep_square));
            self.ep_square = squares::NONE;
        }
        self.halfmove_clock += 1;
        self.zobrist ^= keys().side;
        self.side = self.side.flip();
        if self.side == Color::White { self.fullmove += 1; }
        self.history.push(irr);
    }

    pub fn unmake_null_move(&mut self) {
        let irr = self.history.pop().expect("unmake_null_move: empty history");
        self.side = self.side.flip();
        if self.side == Color::Black { self.fullmove -= 1; }
        self.ep_square = irr.ep_square;
        self.halfmove_clock = irr.halfmove_clock;
        self.zobrist = irr.zobrist;
    }

    /// True if the current position is a draw by repetition (2-fold in search).
    pub fn is_repetition(&self) -> bool {
        if self.halfmove_clock < 4 { return false; }
        let key = self.zobrist;
        let len = self.history.len();
        let lookback = (self.halfmove_clock as usize).min(len);
        let mut count = 0usize;
        for irr in self.history[len - lookback..].iter().rev() {
            if irr.zobrist == key {
                count += 1;
                if count >= 1 { return true; } // 2-fold counts as draw in search
            }
            if irr.halfmove_clock == 0 { break; }
        }
        false
    }

    pub fn make_move(&mut self, mv: Move) {
        let irr = Irreversible {
            castling: self.castling,
            ep_square: self.ep_square,
            halfmove_clock: self.halfmove_clock,
            zobrist: self.zobrist,
            captured_piece: None,
        };

        // Remove old ep and castling keys from zobrist
        if self.ep_square != squares::NONE {
            self.zobrist ^= keys().ep_key(file_of(self.ep_square));
        }
        self.zobrist ^= keys().castling_key(self.castling);

        let from = mv.from();
        let to = mv.to();
        let flags = mv.flags();
        let us = self.side;
        let them = us.flip();

        let (_, moving_piece) = self.piece_at(from).expect("make_move: no piece at from");

        self.halfmove_clock += 1;
        self.ep_square = squares::NONE;

        let mut captured_piece = None;

        match flags {
            FLAG_QUIET => {
                self.move_piece(us, moving_piece, from, to);
                if moving_piece == Piece::Pawn {
                    self.halfmove_clock = 0;
                }
            }
            FLAG_DOUBLE_PUSH => {
                self.move_piece(us, Piece::Pawn, from, to);
                self.halfmove_clock = 0;
                let ep = if us == Color::White { to - 8 } else { to + 8 };
                self.ep_square = ep;
                self.zobrist ^= keys().ep_key(file_of(ep));
            }
            FLAG_CASTLE_KS => {
                let (rook_from, rook_to) = if us == Color::White { (7, 5) } else { (63, 61) };
                self.move_piece(us, Piece::King, from, to);
                self.move_piece(us, Piece::Rook, rook_from, rook_to);
            }
            FLAG_CASTLE_QS => {
                let (rook_from, rook_to) = if us == Color::White { (0, 3) } else { (56, 59) };
                self.move_piece(us, Piece::King, from, to);
                self.move_piece(us, Piece::Rook, rook_from, rook_to);
            }
            FLAG_CAPTURE => {
                let (_, cap) = self.piece_at(to).expect("capture: no piece at to");
                self.remove_piece(them, cap, to);
                self.move_piece(us, moving_piece, from, to);
                self.halfmove_clock = 0;
                captured_piece = Some(cap);
                if moving_piece == Piece::Pawn {
                    self.halfmove_clock = 0;
                }
            }
            FLAG_EP => {
                let cap_sq = if us == Color::White { to - 8 } else { to + 8 };
                self.remove_piece(them, Piece::Pawn, cap_sq);
                self.move_piece(us, Piece::Pawn, from, to);
                self.halfmove_clock = 0;
                captured_piece = Some(Piece::Pawn);
            }
            FLAG_PROMO_N | FLAG_PROMO_B | FLAG_PROMO_R | FLAG_PROMO_Q => {
                let promo = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen]
                    [(flags & 3) as usize];
                self.remove_piece(us, Piece::Pawn, from);
                self.place_piece(us, promo, to);
                self.halfmove_clock = 0;
            }
            FLAG_PROMO_CAPTURE_N | FLAG_PROMO_CAPTURE_B | FLAG_PROMO_CAPTURE_R
            | FLAG_PROMO_CAPTURE_Q => {
                let promo = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen]
                    [(flags & 3) as usize];
                let (_, cap) = self.piece_at(to).expect("promo-capture: no piece at to");
                self.remove_piece(them, cap, to);
                self.remove_piece(us, Piece::Pawn, from);
                self.place_piece(us, promo, to);
                self.halfmove_clock = 0;
                captured_piece = Some(cap);
            }
            _ => unreachable!("Unknown move flag: {}", flags),
        }

        // Update castling rights
        let castling_update = CASTLING_RIGHTS_MASK[from as usize] & CASTLING_RIGHTS_MASK[to as usize];
        self.castling &= castling_update;
        self.zobrist ^= keys().castling_key(self.castling);

        // Flip side
        self.zobrist ^= keys().side;
        self.side = them;

        if self.side == Color::White {
            self.fullmove += 1;
        }

        // Store captured piece in history
        let mut irr = irr;
        irr.captured_piece = captured_piece;
        self.history.push(irr);
    }

    pub fn unmake_move(&mut self, mv: Move) {
        let irr = self.history.pop().expect("unmake_move: empty history");

        self.side = self.side.flip();
        if self.side == Color::Black {
            self.fullmove -= 1;
        }

        let from = mv.from();
        let to = mv.to();
        let flags = mv.flags();
        let us = self.side;
        let them = us.flip();

        match flags {
            FLAG_QUIET => {
                let (_, piece) = self.piece_at(to).unwrap();
                self.move_piece(us, piece, to, from);
            }
            FLAG_DOUBLE_PUSH => {
                self.move_piece(us, Piece::Pawn, to, from);
            }
            FLAG_CASTLE_KS => {
                let (rook_from, rook_to) = if us == Color::White { (7, 5) } else { (63, 61) };
                self.move_piece(us, Piece::King, to, from);
                self.move_piece(us, Piece::Rook, rook_to, rook_from);
            }
            FLAG_CASTLE_QS => {
                let (rook_from, rook_to) = if us == Color::White { (0, 3) } else { (56, 59) };
                self.move_piece(us, Piece::King, to, from);
                self.move_piece(us, Piece::Rook, rook_to, rook_from);
            }
            FLAG_CAPTURE => {
                let (_, piece) = self.piece_at(to).unwrap();
                self.move_piece(us, piece, to, from);
                self.place_piece(them, irr.captured_piece.unwrap(), to);
            }
            FLAG_EP => {
                self.move_piece(us, Piece::Pawn, to, from);
                let cap_sq = if us == Color::White { to - 8 } else { to + 8 };
                self.place_piece(them, Piece::Pawn, cap_sq);
            }
            FLAG_PROMO_N | FLAG_PROMO_B | FLAG_PROMO_R | FLAG_PROMO_Q => {
                let promo = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen]
                    [(flags & 3) as usize];
                self.remove_piece(us, promo, to);
                self.place_piece(us, Piece::Pawn, from);
            }
            FLAG_PROMO_CAPTURE_N | FLAG_PROMO_CAPTURE_B | FLAG_PROMO_CAPTURE_R
            | FLAG_PROMO_CAPTURE_Q => {
                let promo = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen]
                    [(flags & 3) as usize];
                self.remove_piece(us, promo, to);
                self.place_piece(us, Piece::Pawn, from);
                self.place_piece(them, irr.captured_piece.unwrap(), to);
            }
            _ => unreachable!(),
        }

        self.castling = irr.castling;
        self.ep_square = irr.ep_square;
        self.halfmove_clock = irr.halfmove_clock;
        self.zobrist = irr.zobrist;
    }

    /// Parse a FEN string.
    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let mut pos = Position::new();
        let mut parts = fen.split_whitespace();

        let board_str = parts.next().ok_or("missing board")?;
        let side_str = parts.next().ok_or("missing side")?;
        let castle_str = parts.next().ok_or("missing castling")?;
        let ep_str = parts.next().ok_or("missing ep")?;
        let hm_str = parts.next().unwrap_or("0");
        let fm_str = parts.next().unwrap_or("1");

        // Board
        let mut rank = 7i32;
        let mut file = 0i32;
        for ch in board_str.chars() {
            if ch == '/' {
                rank -= 1;
                file = 0;
                continue;
            }
            if let Some(d) = ch.to_digit(10) {
                file += d as i32;
                continue;
            }
            let (color, piece) = match ch {
                'P' => (Color::White, Piece::Pawn),
                'N' => (Color::White, Piece::Knight),
                'B' => (Color::White, Piece::Bishop),
                'R' => (Color::White, Piece::Rook),
                'Q' => (Color::White, Piece::Queen),
                'K' => (Color::White, Piece::King),
                'p' => (Color::Black, Piece::Pawn),
                'n' => (Color::Black, Piece::Knight),
                'b' => (Color::Black, Piece::Bishop),
                'r' => (Color::Black, Piece::Rook),
                'q' => (Color::Black, Piece::Queen),
                'k' => (Color::Black, Piece::King),
                _ => return Err(format!("unknown FEN char: {}", ch)),
            };
            let sq = make_square(rank as u32, file as u32);
            pos.place_piece(color, piece, sq);
            file += 1;
        }

        // Side to move
        pos.side = match side_str {
            "w" => Color::White,
            "b" => {
                pos.zobrist ^= keys().side;
                Color::Black
            }
            _ => return Err("invalid side".into()),
        };

        // Castling rights (zobrist already 0, so just XOR in the final value)
        let mut castling = 0u8;
        for ch in castle_str.chars() {
            match ch {
                'K' => castling |= CASTLE_WK,
                'Q' => castling |= CASTLE_WQ,
                'k' => castling |= CASTLE_BK,
                'q' => castling |= CASTLE_BQ,
                '-' => {}
                _ => {}
            }
        }
        pos.castling = castling;
        pos.zobrist ^= keys().castling_key(castling);

        // En passant
        if ep_str != "-" {
            let sq = parse_sq(ep_str).ok_or("invalid ep square")?;
            pos.ep_square = sq;
            pos.zobrist ^= keys().ep_key(file_of(sq));
        }

        pos.halfmove_clock = hm_str.parse().unwrap_or(0);
        pos.fullmove = fm_str.parse().unwrap_or(1);

        Ok(pos)
    }

    pub fn startpos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn to_fen(&self) -> String {
        let mut fen = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let sq = make_square(rank, file);
                match self.piece_at(sq) {
                    None => empty += 1,
                    Some((c, p)) => {
                        if empty > 0 {
                            fen.push(char::from_digit(empty, 10).unwrap());
                            empty = 0;
                        }
                        let ch = match (c, p) {
                            (Color::White, Piece::Pawn) => 'P',
                            (Color::White, Piece::Knight) => 'N',
                            (Color::White, Piece::Bishop) => 'B',
                            (Color::White, Piece::Rook) => 'R',
                            (Color::White, Piece::Queen) => 'Q',
                            (Color::White, Piece::King) => 'K',
                            (Color::Black, Piece::Pawn) => 'p',
                            (Color::Black, Piece::Knight) => 'n',
                            (Color::Black, Piece::Bishop) => 'b',
                            (Color::Black, Piece::Rook) => 'r',
                            (Color::Black, Piece::Queen) => 'q',
                            (Color::Black, Piece::King) => 'k',
                        };
                        fen.push(ch);
                    }
                }
            }
            if empty > 0 {
                fen.push(char::from_digit(empty, 10).unwrap());
            }
            if rank > 0 {
                fen.push('/');
            }
        }
        fen.push(' ');
        fen.push(if self.side == Color::White { 'w' } else { 'b' });
        fen.push(' ');
        let mut c = String::new();
        if self.castling & CASTLE_WK != 0 { c.push('K'); }
        if self.castling & CASTLE_WQ != 0 { c.push('Q'); }
        if self.castling & CASTLE_BK != 0 { c.push('k'); }
        if self.castling & CASTLE_BQ != 0 { c.push('q'); }
        if c.is_empty() { c.push('-'); }
        fen.push_str(&c);
        fen.push(' ');
        if self.ep_square == squares::NONE {
            fen.push('-');
        } else {
            fen.push_str(&sq_name(self.ep_square));
        }
        fen.push_str(&format!(" {} {}", self.halfmove_clock, self.fullmove));
        fen
    }

    /// Validate board state consistency. Returns None if valid, Some(err) if not.
pub fn validate(&self) -> Option<String> {
    let expected_all = self.occupancy[0] | self.occupancy[1];
    if self.all != expected_all {
        return Some(format!(
            "all mismatch: have {:016x}, expect {:016x}",
            self.all, expected_all
        ));
    }
    if self.occupancy[0] & self.occupancy[1] != 0 {
        return Some(format!(
            "occupancy overlap: {:016x}",
            self.occupancy[0] & self.occupancy[1]
        ));
    }
    for c in 0..2 {
        let mut occ = 0u64;
        for p in 0..6 {
            occ |= self.pieces[c][p];
        }
        if occ != self.occupancy[c] {
            return Some(format!(
                "occupancy[{}] mismatch: have {:016x}, union of pieces = {:016x}",
                c, self.occupancy[c], occ
            ));
        }
    }
    for sq in 0u32..64 {
        let raw = self.mailbox[sq as usize];
        if raw == 0xFF {
            for c in 0..2 {
                for p in 0..6 {
                    if self.pieces[c][p] & (1u64 << sq) != 0 {
                        return Some(format!(
                            "sq {} empty in mailbox but in pieces[{}][{}]",
                            sq, c, p
                        ));
                    }
                }
            }
        } else {
            let c = (raw >> 3) as usize;
            let p = (raw & 7) as usize;
            if p >= 6 || c >= 2 {
                return Some(format!("invalid mailbox byte {:02x} at sq {}", raw, sq));
            }
            if self.pieces[c][p] & (1u64 << sq) == 0 {
                return Some(format!(
                    "sq {} has mailbox c={} p={} but not in pieces bitboard",
                    sq, c, p
                ));
            }
        }
    }
    None
    }
}

/// Castling rights mask per square: if king or rook moves from/to this square, revoke these bits.
pub const CASTLING_RIGHTS_MASK: [u8; 64] = {
    let mut mask = [0xFFu8; 64];
    mask[squares::E1 as usize] &= !(CASTLE_WK | CASTLE_WQ);
    mask[squares::H1 as usize] &= !CASTLE_WK;
    mask[squares::A1 as usize] &= !CASTLE_WQ;
    mask[squares::E8 as usize] &= !(CASTLE_BK | CASTLE_BQ);
    mask[squares::H8 as usize] &= !CASTLE_BK;
    mask[squares::A8 as usize] &= !CASTLE_BQ;
    mask
};
