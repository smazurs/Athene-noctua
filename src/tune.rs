//! Texel tuning: coordinate-descent optimization of eval parameters.
//!
//! Usage: athene-noctua tune <dataset.epd>
//!
//! Dataset format (one position per line):
//!   FEN [result]   where result is "1.0", "0.5", or "0.0"
//!   or:  FEN 1-0 / FEN 0-1 / FEN 1/2-1/2
//!
//! Example datasets:
//!   - Zurichess quiet-labeled: https://github.com/easychessanimations/zurichess
//!   - Generate with: cutechess-cli ... -pgnout games.pgn, then convert PGNs

use crate::board::position::Position;
use crate::eval::evaluate;
use std::fs::File;
use std::io::{BufRead, BufReader};

const K: f64 = 0.45; // sigmoid scaling (tune this first if needed)

fn sigmoid(eval_cp: f64) -> f64 {
    1.0 / (1.0 + 10.0_f64.powf(-K * eval_cp / 400.0))
}

fn compute_error(positions: &[(Position, f64)]) -> f64 {
    let mut total = 0.0f64;
    for (pos, result) in positions {
        let mut p = pos.clone();
        let ev = evaluate(&p) as f64;
        let pred = sigmoid(ev);
        let diff = result - pred;
        total += diff * diff;
    }
    total / positions.len() as f64
}

fn load_dataset(path: &str) -> Vec<(Position, f64)> {
    let file = File::open(path).unwrap_or_else(|e| panic!("Cannot open {}: {}", path, e));
    let reader = BufReader::new(file);
    let mut out = Vec::new();

    for line in reader.lines() {
        let line = line.unwrap();
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        // Try to split off the result token (last bracket or bare token)
        let (fen, result_opt) = if let Some(idx) = line.rfind('[') {
            let fen = line[..idx].trim();
            let rest = &line[idx+1..];
            let result = rest.trim_end_matches(']').trim();
            (fen, parse_result(result))
        } else {
            // last whitespace-delimited token is the result
            let mut parts = line.rsplitn(2, ' ');
            let result_str = parts.next().unwrap_or("");
            let fen = parts.next().unwrap_or("").trim();
            (fen, parse_result(result_str))
        };

        let result = match result_opt {
            Some(r) => r,
            None => continue,
        };

        match Position::from_fen(fen) {
            Ok(pos) => out.push((pos, result)),
            Err(_) => continue,
        }
    }
    out
}

fn parse_result(s: &str) -> Option<f64> {
    match s.trim() {
        "1-0" | "1.0" | "1" => Some(1.0),
        "0-1" | "0.0" | "0" => Some(0.0),
        "1/2-1/2" | "0.5" => Some(0.5),
        _ => None,
    }
}

pub fn run_tune(dataset_path: &str) {
    println!("Loading dataset from {}...", dataset_path);
    let positions = load_dataset(dataset_path);
    if positions.is_empty() {
        eprintln!("No positions loaded. Check dataset format.");
        return;
    }
    println!("Loaded {} positions.", positions.len());

    let base_error = compute_error(&positions);
    println!("Baseline MSE error: {:.8}", base_error);
    println!();
    println!("To tune parameters, the eval weights must be exposed as mutable values.");
    println!("Current error measurement is working. Tuning loop not yet wired to eval constants.");
    println!("Next step: run this with a large dataset (100k+ positions) to measure eval quality.");
    println!("A lower MSE means the eval correlates better with game outcomes.");
}
