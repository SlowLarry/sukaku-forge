use std::env;
use std::io::{self, BufRead, BufWriter, Write};
use std::process::ExitCode;

use sukaku_forge_classic_rater::ClassicRater;
use sukaku_forge_engine::Se121Rating;

const DEFAULT_FORMAT: &str = "%r/%p/%d";

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
    let mut format = DEFAULT_FORMAT.to_owned();
    let mut allow_uniqueness = false;
    let mut positional = None;
    for argument in env::args().skip(1) {
        if argument == "--version" || argument == "-V" {
            println!("sukaku-forge-rate {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if argument == "--help" || argument == "-h" {
            println!("{}", usage());
            return Ok(());
        }
        if let Some(value) = argument.strip_prefix("--format=") {
            validate_format(value)?;
            format = value.to_owned();
            continue;
        }
        if argument == "--input=-" {
            continue;
        }
        if argument == "--allow-uniqueness" {
            allow_uniqueness = true;
            continue;
        }
        if argument.starts_with('-') {
            return Err(format!("unsupported option {argument}\n{}", usage()));
        }
        if positional.replace(argument).is_some() {
            return Err(format!("only one puzzle may be supplied\n{}", usage()));
        }
    }
    validate_format(&format)?;

    let rater = ClassicRater::new().with_uniqueness(allow_uniqueness);
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    if let Some(puzzle) = positional {
        rate_and_write(&rater, &mut output, &format, &puzzle, None)?;
        return output
            .flush()
            .map_err(|error| format!("failed to flush stdout: {error}"));
    }

    let stdin = io::stdin();
    rate_lines(&rater, stdin.lock(), &mut output, &format)?;
    output
        .flush()
        .map_err(|error| format!("failed to flush stdout: {error}"))
}

fn rate_lines(
    rater: &ClassicRater,
    input: impl BufRead,
    output: &mut impl Write,
    format: &str,
) -> Result<(), String> {
    let mut count = 0_usize;
    for (line_index, line) in input.lines().enumerate() {
        let line = line.map_err(|error| format!("failed to read stdin: {error}"))?;
        let puzzle = line.trim();
        if puzzle.is_empty() {
            continue;
        }
        rate_and_write(rater, output, format, puzzle, Some(line_index + 1))?;
        count += 1;
    }
    if count == 0 {
        return Err("stdin contained no puzzles".to_owned());
    }
    Ok(())
}

fn rate_and_write(
    rater: &ClassicRater,
    output: &mut impl Write,
    format: &str,
    puzzle: &str,
    line_number: Option<usize>,
) -> Result<(), String> {
    let rating = rater.rate_text(puzzle).map_err(|error| {
        line_number.map_or_else(
            || error.to_string(),
            |line| format!("puzzle on input line {line}: {error}"),
        )
    })?;
    write_formatted(output, format, puzzle, rating)
        .map_err(|error| format!("failed to write stdout: {error}"))?;
    output
        .write_all(b"\n")
        .map_err(|error| format!("failed to write stdout: {error}"))
}

fn validate_format(format: &str) -> Result<(), String> {
    let mut characters = format.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }
        let Some(token) = characters.next() else {
            return Err("format ends with an incomplete '%' token".to_owned());
        };
        if !matches!(token, '%' | 'g' | 'r' | 'p' | 'd') {
            return Err(format!("unsupported format token %{token}"));
        }
    }
    Ok(())
}

fn write_formatted(
    output: &mut impl Write,
    format: &str,
    puzzle: &str,
    rating: Se121Rating,
) -> io::Result<()> {
    let mut characters = format.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            write!(output, "{character}")?;
            continue;
        }
        match characters.next().expect("validated format") {
            '%' => output.write_all(b"%")?,
            'g' => output.write_all(puzzle.as_bytes())?,
            'r' => write!(output, "{}", rating.er())?,
            'p' => write!(output, "{}", rating.ep())?,
            'd' => write!(output, "{}", rating.ed())?,
            _ => unreachable!("validated format token"),
        }
    }
    Ok(())
}

fn usage() -> &'static str {
    "usage: sukaku-forge-rate [--format=FORMAT] [--input=-] [--allow-uniqueness] [PUZZLE81]\n\
     rates Classic 9x9 puzzles with the corrected SE 1.2.1-derived schedule\n\
     Unique Loops and BUG are disabled unless --allow-uniqueness is supplied"
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_FORMAT, rate_lines, usage, validate_format, write_formatted};
    use sukaku_forge_classic_rater::ClassicRater;
    use sukaku_forge_engine::{Rating, Se121Rating};

    const SOLVED: &str =
        "123456789456789123789123456214365897365897214897214365531642978642978531978531642";

    #[test]
    fn formatter_supports_the_serate_rating_tokens() {
        let rating = Se121Rating::new(
            Rating::from_tenths(89),
            Rating::from_tenths(15),
            Rating::from_tenths(12),
        );
        let mut output = Vec::new();
        write_formatted(&mut output, "%r/%p/%d %% %g", SOLVED, rating).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("8.9/1.5/1.2 % {SOLVED}")
        );
        assert!(validate_format("%x").is_err());
        assert!(validate_format("trailing%").is_err());
    }

    #[test]
    fn batch_skips_blank_lines_and_preserves_input_order() {
        let rater = ClassicRater::new();
        let input = format!("\n{SOLVED}\r\n\n{SOLVED}\n");
        let mut output = Vec::new();
        rate_lines(&rater, input.as_bytes(), &mut output, "%r/%p/%d").unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "0.0/0.0/0.0\n0.0/0.0/0.0\n"
        );
    }

    #[test]
    fn default_format_matches_serate() {
        assert_eq!(DEFAULT_FORMAT, "%r/%p/%d");
    }

    #[test]
    fn help_documents_the_explicit_uniqueness_opt_in() {
        let help = usage();
        assert!(help.contains("--allow-uniqueness"));
        assert!(help.contains("disabled unless"));
    }
}
