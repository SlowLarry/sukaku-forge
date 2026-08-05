use std::env;
use std::io::{self, BufRead, BufWriter, Read, Write};
use std::process::ExitCode;
use std::sync::Arc;

use sukaku_forge_core::{ConstraintTopology, Grid, NonConsecutiveMode, Puzzle, VariantConfig};
use sukaku_forge_engine::{
    EngineConfig, Evidence, RatingMode, RatingResult, RatingTracker, SearchOutcome, SearchPolicy,
    Solver, find_hidden_single,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return Err(usage());
    };
    if command == "--version" || command == "-V" {
        println!("sukaku-forge {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if !matches!(
        command.as_str(),
        "inspect" | "hidden" | "trace" | "rate" | "batch-rate" | "next"
    ) {
        return Err(usage());
    }

    let mut variant = VariantConfig::default();
    let mut engine = EngineConfig::default();
    let mut selected_search_policy = None;
    let mut quiet = false;
    let mut positional = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--anti-knight" || argument == "--isAntiKnight=1" {
            variant.anti_knight = true;
        } else if argument == "--no-blocks" || argument == "--isBlocks=0" {
            variant.blocks = false;
        } else if argument == "--variant-latin" {
            engine.variant_latin = true;
        } else if argument == "--revised"
            || argument == "--revised-rating=1"
            || argument == "--revisedRating=1"
        {
            engine.rating_mode = RatingMode::Revised;
        } else if argument == "--revised-rating=0" || argument == "--revisedRating=0" {
            engine.rating_mode = RatingMode::Original;
        } else if argument == "--forge" || argument == "--search-policy=forge" {
            if selected_search_policy
                .replace(SearchPolicy::Forge)
                .is_some()
            {
                return Err("search policy may only be selected once".to_owned());
            }
            engine.search_policy = SearchPolicy::Forge;
        } else if argument == "--java-compatible"
            || argument == "--search-policy=java"
            || argument == "--search-policy=compatibility"
        {
            if selected_search_policy
                .replace(SearchPolicy::Compatibility)
                .is_some()
            {
                return Err("search policy may only be selected once".to_owned());
            }
            engine.search_policy = SearchPolicy::Compatibility;
        } else if argument == "-P" {
            let value = arguments
                .next()
                .ok_or_else(|| "-P requires 0, 1, or 2".to_owned())?;
            engine.forcing_chain_plus = parse_forcing_chain_plus(&value)?;
        } else if let Some(value) = argument
            .strip_prefix("--FCPlus=")
            .or_else(|| argument.strip_prefix("--fc-plus="))
            .or_else(|| argument.strip_prefix("-P="))
            .or_else(|| argument.strip_prefix("-P"))
        {
            engine.forcing_chain_plus = parse_forcing_chain_plus(value)?;
        } else if argument == "--islkSudokuURUL=1" {
            engine.unique_loop_fix = true;
        } else if argument == "--islkSudokuURUL=0" {
            engine.unique_loop_fix = false;
        } else if argument == "--islkSudokuBUG=1" {
            engine.bug_fix = true;
        } else if argument == "--islkSudokuBUG=0" {
            engine.bug_fix = false;
        } else if argument == "--quiet" {
            quiet = true;
        } else if let Some(value) = argument.strip_prefix("--isNC=") {
            variant.non_consecutive = match value {
                "0" => NonConsecutiveMode::Off,
                "1" => NonConsecutiveMode::Orthogonal,
                "2" => NonConsecutiveMode::OrthogonalCyclic,
                "3" => NonConsecutiveMode::Diagonal,
                "4" => NonConsecutiveMode::DiagonalCyclic,
                _ => return Err(format!("unsupported non-consecutive mode {value}")),
            };
            variant.forbidden_pairs = variant.non_consecutive != NonConsecutiveMode::Off;
        } else if argument.starts_with('-') {
            return Err(format!("unsupported option {argument}"));
        } else {
            positional.push(argument);
        }
    }

    if command == "next" {
        if positional.len() != 2 {
            return Err("next requires VALUES81 and CANDIDATES729".to_owned());
        }
        return replay_next(variant, engine, &positional[0], &positional[1]);
    }

    if command == "batch-rate" {
        if !positional.is_empty() {
            return Err("batch-rate reads one puzzle per nonempty stdin line".to_owned());
        }
        return batch_rate(io::stdin().lock(), variant, engine, quiet);
    }

    let puzzle_text = match positional.len() {
        0 => {
            let mut value = String::new();
            io::stdin()
                .read_to_string(&mut value)
                .map_err(|error| format!("failed to read stdin: {error}"))?;
            value
        }
        1 => positional.pop().expect("one positional argument"),
        _ => return Err("only one puzzle may be supplied".to_owned()),
    };
    let puzzle = Puzzle::parse(&puzzle_text).map_err(|error| error.to_string())?;
    let topology = Arc::new(ConstraintTopology::new(variant));
    let mut grid = Grid::from_puzzle(topology, &puzzle);

    match command.as_str() {
        "inspect" => inspect(&grid, variant),
        "trace" => trace_solve(&mut grid, engine),
        "rate" => rate_grid(grid, engine).map(|result| println!("{result}")),
        "hidden" => hidden_solve(&mut grid, engine),
        _ => unreachable!("command was validated"),
    }
}

fn parse_forcing_chain_plus(value: &str) -> Result<u8, String> {
    let parsed = value
        .parse::<u8>()
        .map_err(|_| format!("unsupported FCPlus value {value}"))?;
    if parsed > 2 {
        return Err(format!(
            "unsupported FCPlus value {value}; expected 0, 1, or 2"
        ));
    }
    Ok(parsed)
}

fn batch_rate(
    input: impl BufRead,
    variant: VariantConfig,
    config: EngineConfig,
    quiet: bool,
) -> Result<(), String> {
    let topology = Arc::new(ConstraintTopology::new(variant));
    let solver = Solver::new(config);
    let mut output = BufWriter::new(io::stdout().lock());
    let mut count = 0_usize;
    for (line_index, line) in input.lines().enumerate() {
        let line = line.map_err(|error| format!("failed to read stdin: {error}"))?;
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let puzzle = Puzzle::parse(text)
            .map_err(|error| format!("puzzle on input line {}: {error}", line_index + 1))?;
        let grid = Grid::from_puzzle(Arc::clone(&topology), &puzzle);
        let result = rate_with_solver(&solver, grid)?;
        if !quiet {
            writeln!(output, "{}", format_result(&result))
                .map_err(|error| format!("failed to write stdout: {error}"))?;
        }
        count += 1;
    }
    if count == 0 {
        return Err("batch-rate received no puzzles".to_owned());
    }
    Ok(())
}

fn rate_grid(grid: Grid, config: EngineConfig) -> Result<String, String> {
    rate_with_solver(&Solver::new(config), grid).map(|result| format_result(&result))
}

fn rate_with_solver(solver: &Solver, mut grid: Grid) -> Result<RatingResult, String> {
    let mut tracker = RatingTracker::default();
    loop {
        match solver.next_inference(&grid) {
            SearchOutcome::Found(inference) => {
                tracker.observe(&inference);
                inference.apply(&mut grid);
            }
            SearchOutcome::None => return Ok(tracker.result()),
            SearchOutcome::Incomplete(gap) => return Err(format!("rating stopped at {gap}")),
        }
    }
}

fn format_result(result: &RatingResult) -> String {
    format!(
        "RESULT\t{}\t{}\t{}\t{}\t{}\t{}",
        result.er().rating(),
        result.ep().rating(),
        result.ed().rating(),
        result.er().name(),
        result.ep().name(),
        result.ed().name()
    )
}

fn inspect(grid: &Grid, variant: VariantConfig) -> Result<(), String> {
    println!("version={}", env!("CARGO_PKG_VERSION"));
    println!("anti_knight={}", variant.anti_knight);
    println!("givens={}", grid.givens().count());
    println!("grid={}", grid.values_string());
    println!("candidates={}", grid.candidate_string());
    Ok(())
}

fn hidden_solve(grid: &mut Grid, config: EngineConfig) -> Result<(), String> {
    let mut step = 0_u32;
    while let Some(inference) = find_hidden_single(grid, config) {
        step += 1;
        let alone = matches!(
            inference.evidence(),
            Evidence::HiddenSingle { alone: true, .. }
        );
        println!(
            "{step}: {} {}={} rating={} alone={alone}",
            inference.technique().name(),
            inference.placement_cell().expect("direct placement"),
            inference.placement_digit().expect("direct placement"),
            inference.rating()
        );
        inference.apply(grid);
    }
    println!("grid={}", grid.values_string());
    println!("candidates={}", grid.candidate_string());
    Ok(())
}

fn trace_solve(grid: &mut Grid, config: EngineConfig) -> Result<(), String> {
    let solver = Solver::new(config);
    let mut ratings = RatingTracker::default();
    loop {
        match solver.next_inference(grid) {
            SearchOutcome::Found(inference) => {
                let rating = inference.rating();
                let description = inference.description(grid.topology());
                ratings.observe(&inference);
                inference.apply(grid);
                println!(
                    "STEP\t{rating}\t{description}\t{}\t{}",
                    grid.values_string(),
                    grid.candidate_string()
                );
            }
            SearchOutcome::None => break,
            SearchOutcome::Incomplete(gap) => {
                return Err(format!(
                    "rating stopped at {gap}; grid={} candidates={}",
                    grid.values_string(),
                    grid.candidate_string()
                ));
            }
        }
    }
    let result = ratings.result();
    println!(
        "RESULT\t{}\t{}\t{}\t{}\t{}\t{}",
        result.er().rating(),
        result.ep().rating(),
        result.ed().rating(),
        result.er().name(),
        result.ep().name(),
        result.ed().name()
    );
    Ok(())
}

fn replay_next(
    variant: VariantConfig,
    config: EngineConfig,
    values_text: &str,
    candidates_text: &str,
) -> Result<(), String> {
    let values = Puzzle::parse(values_text).map_err(|error| error.to_string())?;
    let candidates = Puzzle::parse(candidates_text).map_err(|error| error.to_string())?;
    let topology = Arc::new(ConstraintTopology::new(variant));
    let mut grid =
        Grid::from_snapshot(topology, &values, &candidates).map_err(|error| error.to_string())?;
    match Solver::new(config).next_inference(&grid) {
        SearchOutcome::Found(inference) => {
            let rating = inference.rating();
            let description = inference.description(grid.topology());
            inference.apply(&mut grid);
            println!(
                "STEP\t{rating}\t{description}\t{}\t{}",
                grid.values_string(),
                grid.candidate_string()
            );
            Ok(())
        }
        SearchOutcome::None => Err("snapshot is already solved".to_owned()),
        SearchOutcome::Incomplete(gap) => Err(format!("snapshot stopped at {gap}")),
    }
}

fn usage() -> String {
    "usage: sukaku-forge <inspect|hidden|trace|rate> [OPTIONS] [PUZZLE]\n       \
     sukaku-forge batch-rate [OPTIONS] < PUZZLES.txt\n       \
     sukaku-forge next [OPTIONS] VALUES81 CANDIDATES729\n\n       \
     compatibility options:\n       \
       --revised | --revisedRating=1\n       \
       --search-policy=compatibility|forge | --forge\n       \
       --FCPlus=0|1|2 | -P 0|1|2\n       \
       --isNC=0|1|2|3|4"
        .to_owned()
}
