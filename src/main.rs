mod board;
mod eval;
mod movegen;
mod search;
mod tt;
mod uci;

use board::attacks;
use movegen::init_tables;

fn main() {
    // Initialize attack tables (magic bitboards, leaper tables)
    attacks::init();
    init_tables();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("verify") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            let illegal = uci::find_illegal_moves(&mut pos, depth);
            println!("Illegal moves found: {}", illegal);
        }
        Some("movetype") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            let mut counts = [0u64; 16];
            uci::count_move_types(&mut pos, depth, &mut counts);
            let names = ["quiet","dpush","castleKS","castleQS","capture","ep",
                         "6","7","promoN","promoB","promoR","promoQ",
                         "pcN","pcB","pcR","pcQ"];
            for (i, &c) in counts.iter().enumerate() {
                if c > 0 { println!("{}: {}", names[i], c); }
            }
            println!("Total: {}", counts.iter().sum::<u64>());
        }
        Some("crosscheck") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPBBPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            let nodes = uci::perft_crosscheck(&mut pos, depth);
            println!("crosscheck perft({}) = {}", depth, nodes);
        }
        Some("validate_perft") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            let nodes = uci::perft_validating(&mut pos, depth);
            println!("perft_validating({}) = {}", depth, nodes);
        }
        Some("bench") => run_bench(),
        Some("perft") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            let start = std::time::Instant::now();
            let nodes = uci::perft(&mut pos, depth);
            let elapsed = start.elapsed();
            let nps = if elapsed.as_secs_f64() > 0.0 {
                (nodes as f64 / elapsed.as_secs_f64()) as u64
            } else {
                0
            };
            println!(
                "perft({}) = {} ({:.2}s, {} nps)",
                depth,
                nodes,
                elapsed.as_secs_f64(),
                nps
            );
        }
        Some("divide") => {
            let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let fen = args.get(3).map(String::as_str).unwrap_or(
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            );
            let mut pos = board::position::Position::from_fen(fen).unwrap();
            uci::perft_divide(&mut pos, depth);
        }
        _ => uci::run_uci(),
    }
}

fn run_bench() {
    let positions = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ];

    let depth = 5;
    let mut total_nodes = 0u64;
    let start = std::time::Instant::now();

    for fen in &positions {
        let mut pos = board::position::Position::from_fen(fen).unwrap();
        let nodes = uci::perft(&mut pos, depth);
        total_nodes += nodes;
    }

    let elapsed = start.elapsed();
    println!("Bench: {} nodes in {:.2}s", total_nodes, elapsed.as_secs_f64());
}
